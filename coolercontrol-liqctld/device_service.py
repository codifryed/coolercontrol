#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2022  Guy Boldon
#  |
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU General Public License as published by
#  the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#  |
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU General Public License for more details.
#  |
#  You should have received a copy of the GNU General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
# ----------------------------------------------------------------------------------------------------------------------

import logging
from http import HTTPStatus
from typing import List, Tuple, Any, Union

import liquidctl
from fastapi import HTTPException
from liquidctl.driver.asetek import Modern690Lc, Legacy690Lc
from liquidctl.driver.aura_led import AuraLed
from liquidctl.driver.base import BaseDriver
from liquidctl.driver.corsair_hid_psu import CorsairHidPsu

from device_executor import DeviceExecutor
from models import LiquidctlException, Device, Statuses, DeviceProperties

log = logging.getLogger(__name__)


class DeviceService:
    """
    The Service which keeps track of devices and handles all communication
    """

    def __init__(self) -> None:
        """
        To enable synchronous & parallel device communication, we use our own DeviceExecutor
        """
        self.devices: dict[int, BaseDriver] = {}
        # this can be used to set specific flags like legacy/type/special things from settings in coolercontrol
        self.device_infos: dict[int, Any] = {}
        self.device_executor: DeviceExecutor = DeviceExecutor()

    def get_devices(self) -> List[Device]:
        log.info("Getting device list")
        if self.devices:  # if we've already searched for devices, don't do so again, just retrieve device info
            devices = []
            for index_id, lc_device in self.devices.items():
                speed_channel_map = getattr(lc_device, "_speed_channels", {})
                speed_channels = list(speed_channel_map.keys())
                color_channel_map = getattr(lc_device, "_color_channels", {})
                color_channels = list(color_channel_map.keys())
                devices.append(
                    Device(
                        id=index_id, description=lc_device.description,
                        device_type=type(lc_device).__name__, serial_number=lc_device.serial_number,
                        properties=DeviceProperties(speed_channels, color_channels)
                    )
                )
            return devices
        try:  # otherwise find devices
            log.debug_lc("liquidctl.find_liquidctl_devices()")
            devices: List[Device] = []
            found_devices = list(liquidctl.find_liquidctl_devices())
            for index, lc_device in enumerate(found_devices):
                index_id = index + 1
                self.devices[index_id] = lc_device
                speed_channel_map = getattr(lc_device, "_speed_channels", {})
                speed_channels = list(speed_channel_map.keys())
                color_channel_map = getattr(lc_device, "_color_channels", {})
                color_channels = list(color_channel_map.keys())
                devices.append(
                    Device(
                        id=index_id, description=lc_device.description,
                        device_type=type(lc_device).__name__, serial_number=lc_device.serial_number,
                        properties=DeviceProperties(speed_channels, color_channels)
                    )
                )
            self.device_executor.set_number_of_devices(len(devices))
            return devices
        except ValueError:  # ValueError can happen when no devices were found
            log.info('No Liquidctl devices detected')
            return []

    def set_device_as_legacy690(self, device_id: int) -> Device:
        """
        Modern and Legacy Asetek 690Lc devices have the same device ID.
        We ask the user to verify which device is connected so that we can correctly communicate with the device
        """
        if self.devices.get(device_id) is None:
            raise HTTPException(HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found")
        lc_device = self.devices[device_id]
        if isinstance(lc_device, Legacy690Lc):
            log.warning(f"Device #{device_id} is already set as a Legacy690Lc device")
            return Device(
                id=device_id, description=lc_device.description,
                device_type=type(lc_device).__name__, serial_number=lc_device.serial_number,
                properties=DeviceProperties([], [])
            )
        elif not isinstance(lc_device, Modern690Lc):
            message = f"Device #{device_id} is not applicable to be downgraded to a Legacy690Lc"
            log.warning(message)
            raise HTTPException(HTTPStatus.EXPECTATION_FAILED, message)
        log.info(f"Setting device #{device_id} as legacy690")
        log.debug_lc("Legacy690Lc.find_liquidctl_devices()")
        legacy_job = self.device_executor.submit(device_id, Legacy690Lc.find_supported_devices)
        asetek690s = list(legacy_job.result())
        if not asetek690s:
            log.error("Could not find any Legacy690Lc devices. This shouldn't happen")
            raise LiquidctlException("Could not find any Legacy690Lc devices.")
        elif len(asetek690s) > 1:
            # if there are multiple options, we need to find the correlating device
            current_asetek690_ids = [
                asetek690_id
                for asetek690_id, device in self.devices.items()
                if isinstance(device, (Modern690Lc, Legacy690Lc))
            ]
            device_index: int = 0
            for asetek690_id in current_asetek690_ids:
                if asetek690_id == device_id:
                    break
                else:
                    device_index += 1
            self.devices[device_id] = asetek690s[device_index]
            lc_device = self.devices[device_id]
        else:
            self.devices[device_id] = asetek690s[0]
            lc_device = self.devices[device_id]
        return Device(
            id=device_id, description=lc_device.description,
            device_type=type(lc_device).__name__, serial_number=lc_device.serial_number,
            properties=DeviceProperties([], [])
        )

    def connect_devices(self) -> None:
        if not self.devices:
            raise HTTPException(HTTPStatus.BAD_REQUEST, "No Devices found")
        log.info("Connecting to all Liquidctl Devices")
        for device_id, lc_device in self.devices.items():
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.connect() ")
            # currently only smbus devices have options for connect()
            connect_job = self.device_executor.submit(device_id, lc_device.connect)
            connect_job.result()

    def initialize_device(self, device_id: int, init_args: dict[str, str]) -> Statuses:
        if self.devices.get(device_id) is None:
            raise HTTPException(HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found")
        log.info(f"Initializing Liquidctl device #{device_id} with arguments: {init_args}")
        try:
            lc_device = self.devices[device_id]
            if isinstance(lc_device, AuraLed):
                log.info("Skipping AuraLed device initialization, not needed.")
                # also has negative side effects of clearing previously set lighting settings
                return []
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.initialize({init_args}) ")
            init_job = self.device_executor.submit(device_id, lc_device.initialize, **init_args)
            lc_init_status: List[Tuple] = init_job.result()
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}initialize() RESPONSE: {lc_init_status}")
            return self._stringify_status(lc_init_status)
        except OSError as os_exc:  # OSError when device was found but there's a permissions error
            log.error('Device Communication Error', exc_info=os_exc)
            raise LiquidctlException("Unexpected Device Communication Error") from os_exc

    def get_status(self, device_id: int) -> Statuses:
        if self.devices.get(device_id) is None:
            raise HTTPException(HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found")
        log.debug(f"Getting status for device: {device_id}")
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.get_status() ")
            status_job = self.device_executor.submit(device_id, lc_device.get_status)
            status: List[Tuple[str, Union[str, int, float], str]] = status_job.result()
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.get_status() RESPONSE: {status}")
            return self._stringify_status(status)
        except BaseException as err:
            log.error("Error getting status:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def set_fixed_speed(self, device_id: int, speed_kwargs: dict[str, str | int]) -> None:
        if self.devices.get(device_id) is None:
            raise HTTPException(HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found")
        log.debug(f"Setting fixes speed for device: {device_id} with args: {speed_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.set_fixed_speed({speed_kwargs}) ")
            status_job = self.device_executor.submit(device_id, lc_device.set_fixed_speed, **speed_kwargs)
            status_job.result()
        except BaseException as err:
            log.error("Error setting fixed speed:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def set_speed_profile(self, device_id: int, speed_kwargs: dict[str, Any]) -> None:
        if self.devices.get(device_id) is None:
            raise HTTPException(HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found")
        log.debug(f"Setting speed profile for device: {device_id} with args: {speed_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.set_speed_profile({speed_kwargs}) ")
            status_job = self.device_executor.submit(device_id, lc_device.set_speed_profile, **speed_kwargs)
            status_job.result()
        except BaseException as err:
            log.error("Error setting speed profile:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def set_color(self, device_id: int, color_kwargs: dict[str, Any]) -> None:
        if self.devices.get(device_id) is None:
            raise HTTPException(HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found")
        log.debug(f"Setting color for device: {device_id} with args: {color_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.set_color({color_kwargs}) ")
            status_job = self.device_executor.submit(device_id, lc_device.set_color, **color_kwargs)
            status_job.result()
        except BaseException as err:
            log.error("Error setting color:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def set_screen(self, device_id: int, screen_kwargs: dict[str, str]) -> None:
        if self.devices.get(device_id) is None:
            raise HTTPException(HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found")
        log.debug(f"Setting screen for device: {device_id} with args: {screen_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.set_screen({screen_kwargs}) ")
            status_job = self.device_executor.submit(device_id, lc_device.set_screen, **screen_kwargs)
            status_job.result()
        except BaseException as err:
            log.error("Error setting screen:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def disconnect_all(self) -> None:
        for device_id, lc_device in self.devices.items():
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.disconnect() ")
            disconnect_job = self.device_executor.submit(device_id, lc_device.disconnect)
            disconnect_job.result()
        self.devices.clear()

    def shutdown(self) -> None:
        for device_id, lc_device in self.devices.items():
            if isinstance(lc_device, CorsairHidPsu):  # attempt to reset fan control back to hardware
                log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.initialize() ")
                init_job = self.device_executor.submit(device_id, lc_device.initialize)
                init_job.result()
        self.disconnect_all()
        self.device_executor.shutdown()

    @staticmethod
    def _stringify_status(
            statuses: List[Tuple[str, Union[str, int, float], str]]
    ) -> Statuses:
        return [(str(status[0]), str(status[1]), str(status[2])) for status in statuses]
