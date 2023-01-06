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
from test_service_ext import TestServiceExtension, ENABLE_MOCKS

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
            devices = [
                Device(
                    id=index_id,
                    description=lc_device.description,
                    device_type=type(lc_device).__name__,
                    serial_number=lc_device.serial_number,
                    properties=self._get_device_properties(lc_device),
                )
                for index_id, lc_device in self.devices.items()
            ]
            return devices
        try:  # otherwise find devices
            log.debug_lc("liquidctl.find_liquidctl_devices()")
            devices: List[Device] = []
            found_devices = list(liquidctl.find_liquidctl_devices())
            TestServiceExtension.insert_test_mocks(found_devices)
            for index, lc_device in enumerate(found_devices):
                index_id = index + 1
                self.devices[index_id] = lc_device
                devices.append(
                    Device(
                        id=index_id, description=lc_device.description,
                        device_type=type(lc_device).__name__, serial_number=lc_device.serial_number,
                        properties=self._get_device_properties(lc_device)
                    )
                )
            self.device_executor.set_number_of_devices(len(devices))
            return devices
        except ValueError:  # ValueError can happen when no devices were found
            log.info('No Liquidctl devices detected')
            return []

    @staticmethod
    def _get_device_properties(lc_device: BaseDriver) -> DeviceProperties:
        """Get device instance attributes to determine the specific configuration for a given device"""
        # SmartDevice2
        speed_channel_dict = getattr(lc_device, "_speed_channels", {})
        speed_channels = list(speed_channel_dict.keys())
        color_channel_dict = getattr(lc_device, "_color_channels", {})
        color_channels = list(color_channel_dict.keys())
        # Kraken 2
        supports_cooling: bool | None = getattr(lc_device, "supports_cooling", None)
        supports_cooling_profiles: bool | None = getattr(lc_device, "supports_cooling_profiles", None)
        supports_lighting: bool | None = getattr(lc_device, "supports_lighting", None)
        # Aquacomputer
        if not speed_channels:
            device_info_dict = getattr(lc_device, "_device_info", {})
            controllable_pump_and_fans = device_info_dict.get("fan_ctrl", {})
            speed_channels = list(controllable_pump_and_fans.keys())
        # CommanderCore
        if not speed_channels:
            has_pump: bool | None = getattr(lc_device, "_has_pump", None)
            if has_pump is not None and has_pump:
                speed_channels = ["pump"]
        return DeviceProperties(
            speed_channels, color_channels,
            supports_cooling, supports_cooling_profiles, supports_lighting
        )

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
                properties=DeviceProperties()
            )
        elif not isinstance(lc_device, Modern690Lc):
            message = f"Device #{device_id} is not applicable to be downgraded to a Legacy690Lc"
            log.warning(message)
            raise HTTPException(HTTPStatus.EXPECTATION_FAILED, message)
        log.info(f"Setting device #{device_id} as legacy690")
        log.debug_lc("Legacy690Lc.find_liquidctl_devices()")
        if ENABLE_MOCKS:
            asetek690s = [lc_device.downgrade_to_legacy()]
        else:
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
            properties=DeviceProperties()
        )

    def connect_devices(self) -> None:
        if not self.devices:
            raise HTTPException(HTTPStatus.BAD_REQUEST, "No Devices found")
        log.info("Connecting to all Liquidctl Devices")
        for device_id, lc_device in self.devices.items():
            log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.connect() ")
            if ENABLE_MOCKS:
                connect_job = self.device_executor.submit(device_id, TestServiceExtension.connect_mock, lc_device=lc_device)
            else:
                # currently only smbus devices have options for connect()
                connect_job = self.device_executor.submit(device_id, lc_device.connect)
            try:
                connect_job.result()
            except RuntimeError as err:
                if "already open" in str(err):
                    log.warning("%s already connected", lc_device.description)
                else:
                    raise LiquidctlException("Unexpected Device Communication Error") from err

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
            if ENABLE_MOCKS:
                init_job = self.device_executor.submit(device_id, TestServiceExtension.initialize_mock, lc_device=lc_device)
            else:
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
            if ENABLE_MOCKS:
                prepare_mock_job = self.device_executor.submit(
                    device_id,
                    TestServiceExtension.prepare_for_mocks_get_status, lc_device=lc_device
                )
                prepare_mock_job.result()
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
