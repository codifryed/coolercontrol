#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2023  Guy Boldon
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

import concurrent
import logging
import time
from http import HTTPStatus
from typing import Any, Dict, List, Optional, Tuple, Union

import liquidctl
from fastapi import HTTPException

# these imports need to be dynamic for older liquidctl versions:
try:
    from liquidctl.driver.aquacomputer import Aquacomputer
    from liquidctl.driver.asetek_pro import HydroPro
    from liquidctl.driver.aura_led import AuraLed
    from liquidctl.driver.commander_core import CommanderCore
    from liquidctl.driver.smart_device import H1V2
except ImportError:
    Aquacomputer = None
    HydroPro = None
    AuraLed = None
    CommanderCore = None
    H1V2 = None
from coolercontrol_liqctld.device_executor import DeviceExecutor
from coolercontrol_liqctld.models import (
    Device,
    DeviceProperties,
    LiquidctlException,
    Statuses,
)
from liquidctl.driver.asetek import Legacy690Lc, Modern690Lc
from liquidctl.driver.base import BaseDriver
from liquidctl.driver.commander_pro import CommanderPro
from liquidctl.driver.corsair_hid_psu import CorsairHidPsu
from liquidctl.driver.hydro_platinum import HydroPlatinum
from liquidctl.driver.kraken2 import Kraken2
from liquidctl.driver.smart_device import SmartDevice, SmartDevice2

log = logging.getLogger(__name__)

E2E_TESTING_ENABLED: bool = False
DEVICE_TIMEOUT_SECS: float = 9.5
DEVICE_READ_STATUS_TIMEOUT_SECS: float = 0.550


class DeviceService:
    """
    The Service which keeps track of devices and handles all communication
    """

    def __init__(self) -> None:
        """
        To enable synchronous & parallel device communication, we use our own DeviceExecutor
        """
        self.devices: Dict[int, BaseDriver] = {}
        # this can be used to set specific flags like legacy/type/special things from settings in coolercontrol
        self.device_infos: Dict[int, Any] = {}
        self.device_executor: DeviceExecutor = DeviceExecutor()
        self.device_status_cache: Dict[int, Statuses] = {}

    def get_devices(self) -> List[Device]:
        log.info("Getting device list")
        if (
            self.devices
        ):  # if we've already searched for devices, don't do so again, just retrieve device info
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
            if E2E_TESTING_ENABLED:
                from coolercontrol_liqctld.e2e_tests.service_ext import (
                    TestServiceExtension,
                )

                TestServiceExtension.insert_test_mocks(found_devices)
            self.device_executor.set_number_of_devices(len(found_devices))
            for index, lc_device in enumerate(found_devices):
                index_id = index + 1
                self.devices[index_id] = lc_device
                self._connect_device(index_id, lc_device)
                description: str = getattr(lc_device, "description", "")
                serial_number: Optional[str] = getattr(
                    lc_device, "_serial_number", None
                )  # Aquacomputer devices read their serial number
                if not serial_number:
                    try:
                        serial_number = getattr(lc_device, "serial_number", None)
                    except ValueError:
                        log.warning(
                            f"No serial number info found for LC #{index_id} {description}"
                        )
                devices.append(
                    Device(
                        id=index_id,
                        description=description,
                        device_type=type(lc_device).__name__,
                        serial_number=serial_number,
                        properties=self._get_device_properties(lc_device),
                    )
                )
            device_names = [device.description for device in devices]
            log.info(f"Devices found: {device_names}")
            return devices
        except ValueError as ve:  # ValueError can happen when no devices were found
            log.debug("ValueError when trying to find all devices", exc_info=ve)
            log.info("No Liquidctl devices detected")
            return []

    @staticmethod
    def _get_device_properties(lc_device: BaseDriver) -> DeviceProperties:
        """Get device instance attributes to determine the specific configuration for a given device"""
        speed_channels: List[str] = []
        color_channels: List[str] = []
        supports_cooling: Optional[bool] = None
        supports_cooling_profiles: Optional[bool] = None
        supports_lighting: Optional[bool] = None
        led_count: Optional[int] = None
        if isinstance(lc_device, (SmartDevice2, SmartDevice)):
            speed_channel_dict = getattr(lc_device, "_speed_channels", {})
            speed_channels = list(speed_channel_dict.keys())
            color_channel_dict = getattr(lc_device, "_color_channels", {})
            color_channels = list(color_channel_dict.keys())
        elif H1V2 is not None and isinstance(lc_device, H1V2):
            speed_channel_dict = getattr(lc_device, "_speed_channels", {})
            speed_channels = list(speed_channel_dict.keys())
            color_channel_dict = getattr(lc_device, "_color_channels", {})
            color_channels = list(color_channel_dict.keys())
        elif isinstance(lc_device, Kraken2):
            supports_cooling = getattr(lc_device, "supports_cooling", None)
            # this property in particular requires connect() to already have been called:
            supports_cooling_profiles = getattr(
                lc_device, "supports_cooling_profiles", None
            )
            supports_lighting = getattr(lc_device, "supports_lighting", None)
        elif Aquacomputer is not None and isinstance(lc_device, Aquacomputer):
            device_info_dict = getattr(lc_device, "_device_info", {})
            controllable_pump_and_fans = device_info_dict.get("fan_ctrl", {})
            speed_channels = list(controllable_pump_and_fans.keys())
        elif CommanderCore is not None and isinstance(lc_device, CommanderCore):
            if _ := getattr(lc_device, "_has_pump", False):
                speed_channels = ["pump"]
        elif isinstance(lc_device, CommanderPro):
            if fan_names := getattr(lc_device, "_fan_names", []):
                speed_channels = fan_names
            if led_names := getattr(lc_device, "_led_names", []):
                color_channels = led_names
        elif isinstance(lc_device, HydroPlatinum):
            if fan_names := getattr(lc_device, "_fan_names", []):
                speed_channels = fan_names
            if led_count_found := getattr(lc_device, "_led_count", 0):
                color_channels = ["led"]
                led_count = led_count_found
        elif HydroPro is not None and isinstance(lc_device, HydroPro):
            if fan_count := getattr(lc_device, "_fan_count", 0):
                speed_channels = [
                    f"fan{fan_number + 1}" for fan_number in range(fan_count)
                ]
        return DeviceProperties(
            speed_channels=speed_channels,
            color_channels=color_channels,
            supports_cooling=supports_cooling,
            supports_cooling_profiles=supports_cooling_profiles,
            supports_lighting=supports_lighting,
            led_count=led_count,
        )

    def set_device_as_legacy690(self, device_id: int) -> Device:
        """
        Modern and Legacy Asetek 690Lc devices have the same device ID.
        We ask the user to verify which device is connected so that we can correctly communicate with the device
        """
        if self.devices.get(device_id) is None:
            raise HTTPException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        lc_device = self.devices[device_id]
        if isinstance(lc_device, Legacy690Lc):
            log.warning(f"Device #{device_id} is already set as a Legacy690Lc device")
            return Device(
                id=device_id,
                description=lc_device.description,
                device_type=type(lc_device).__name__,
                serial_number=lc_device.serial_number,
                properties=DeviceProperties(),
            )
        elif not isinstance(lc_device, Modern690Lc):
            message = f"Device #{device_id} is not applicable to be downgraded to a Legacy690Lc"
            log.warning(message)
            raise HTTPException(HTTPStatus.EXPECTATION_FAILED, message)
        log.info(f"Setting device #{device_id} as legacy690")
        self._disconnect_device(device_id, lc_device)
        if E2E_TESTING_ENABLED:
            log.debug_lc("Legacy690Lc.downgrade_to_legacy()")
            asetek690s = [lc_device.downgrade_to_legacy()]
        else:
            log.debug_lc("Legacy690Lc.find_liquidctl_devices()")
            legacy_job = self.device_executor.submit(
                device_id, Legacy690Lc.find_supported_devices
            )
            asetek690s = list(legacy_job.result(timeout=DEVICE_TIMEOUT_SECS))
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

        self._connect_device(device_id, lc_device)
        description: str = getattr(lc_device, "description", "")
        serial_number: Optional[str] = None
        try:
            serial_number = getattr(lc_device, "serial_number", None)
        except ValueError:
            log.warning(
                f"No serial number info found for LC #{device_id} {description}"
            )
        return Device(
            id=device_id,
            description=description,
            device_type=type(lc_device).__name__,
            serial_number=serial_number,
            properties=DeviceProperties(),
        )

    def connect_devices(self) -> None:
        if not self.devices:
            raise HTTPException(HTTPStatus.BAD_REQUEST, "No Devices found")
        log.info("Connecting to all Liquidctl Devices")
        for device_id, lc_device in self.devices.items():
            self._connect_device(device_id, lc_device)

    def _connect_device(self, device_id: int, lc_device: BaseDriver) -> None:
        log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.connect() ")
        if E2E_TESTING_ENABLED:
            from coolercontrol_liqctld.e2e_tests.service_ext import TestServiceExtension

            connect_job = self.device_executor.submit(
                device_id, TestServiceExtension.connect_mock, lc_device=lc_device
            )
        else:
            # currently only smbus devices have options for connect()
            connect_job = self.device_executor.submit(device_id, lc_device.connect)
        try:
            connect_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except RuntimeError as err:
            if "already open" in str(err):
                log.warning("%s already connected", lc_device.description)
            else:
                raise LiquidctlException(
                    "Unexpected Device Communication Error"
                ) from err

    def initialize_device(self, device_id: int, init_args: Dict[str, str]) -> Statuses:
        if self.devices.get(device_id) is None:
            raise HTTPException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.info(
            f"Initializing Liquidctl device #{device_id} with arguments: {init_args}"
        )
        try:
            lc_device = self.devices[device_id]
            if AuraLed is not None and isinstance(lc_device, AuraLed):
                log.info("Skipping AuraLed device initialization, not needed.")
                # also has negative side effects of clearing previously set lighting settings
                return []
            log.debug_lc(
                f"LC #{device_id} {lc_device.__class__.__name__}.initialize({init_args}) "
            )
            if E2E_TESTING_ENABLED:
                from coolercontrol_liqctld.e2e_tests.service_ext import (
                    TestServiceExtension,
                )

                init_job = self.device_executor.submit(
                    device_id, TestServiceExtension.initialize_mock, lc_device=lc_device
                )
            else:
                init_job = self.device_executor.submit(
                    device_id, lc_device.initialize, **init_args
                )
            lc_init_status: List[Tuple] = init_job.result(timeout=DEVICE_TIMEOUT_SECS)
            log.debug_lc(
                f"LC #{device_id} {lc_device.__class__.__name__}initialize() RESPONSE: {lc_init_status}"
            )
            return self._stringify_status(lc_init_status)
        except (
            OSError
        ) as os_exc:  # OSError when device was found but there's a permissions error
            log.error("Device Communication Error", exc_info=os_exc)
            raise LiquidctlException(
                "Unexpected Device Communication Error"
            ) from os_exc

    def get_status(self, device_id: int) -> Statuses:
        if self.devices.get(device_id) is None:
            raise HTTPException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Getting status for device: {device_id}")
        try:
            return self._get_current_or_cached_device_status(device_id)
        except BaseException as err:
            log.error("Error getting status:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def _get_current_or_cached_device_status(self, device_id: int) -> Statuses:
        lc_device = self.devices[device_id]
        if E2E_TESTING_ENABLED:
            from coolercontrol_liqctld.e2e_tests.service_ext import TestServiceExtension

            prepare_mock_job = self.device_executor.submit(
                device_id,
                TestServiceExtension.prepare_for_mocks_get_status,
                lc_device=lc_device,
            )
            prepare_mock_job.result(timeout=DEVICE_READ_STATUS_TIMEOUT_SECS)
        log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.get_status() ")
        status_job = self.device_executor.submit(device_id, lc_device.get_status)
        try:
            status: List[Tuple[str, Union[str, int, float], str]] = status_job.result(
                timeout=DEVICE_READ_STATUS_TIMEOUT_SECS
            )
            log.debug_lc(
                f"LC #{device_id} {lc_device.__class__.__name__}.get_status() RESPONSE: {status}"
            )
            serialized_status = self._stringify_status(status)
            self.device_status_cache[device_id] = serialized_status
            return serialized_status
        except concurrent.futures.TimeoutError as te:
            log.debug(
                f"Timeout occurred while trying to get device status for LC #{device_id}. Reusing last status if possible."
            )
            cached_status = self.device_status_cache.get(device_id)
            if self.device_executor.device_queue_empty(
                device_id
            ):  # if emtpy this was likely a device timeout with a single job
                log.debug("Running long-lasting async get_status() call")
                async_status_job = self.device_executor.submit(
                    device_id, self._long_async_status_request, dev_id=device_id
                )
                if cached_status is not None:
                    # return the currently cached status immediately and let the async request above refresh the cache in the background
                    return cached_status
                # else rerun the status request with a very long timeout and wait for the output so that the cache fills up at least once
                try:
                    return async_status_job.result(timeout=DEVICE_TIMEOUT_SECS)
                except concurrent.futures.TimeoutError as te:
                    log.error(
                        f"Unknown issue with device communication and no Status Cache yet filled for device LC #{device_id}"
                    )
                    raise te
                finally:
                    async_status_job.cancel()
            # otherwise this was a future timeout with a job still running in the queue
            if cached_status is None:
                log.error(f"No Status Cache yet filled for device LC #{device_id}")
                raise te
            return cached_status
        finally:
            status_job.cancel()

    def _long_async_status_request(self, dev_id: int) -> Statuses:
        """This function is used to get the status in an async way for devices that have extreme latency"""
        lc_device = self.devices[dev_id]
        log.debug_lc(f"LC #{dev_id} {lc_device.__class__.__name__}.get_status() ")
        status = lc_device.get_status()
        log.debug_lc(
            f"LC #{dev_id} {lc_device.__class__.__name__}.get_status() RESPONSE: {status}"
        )
        serialized_status = self._stringify_status(status)
        self.device_status_cache[dev_id] = serialized_status
        return serialized_status

    def set_fixed_speed(
        self, device_id: int, speed_kwargs: Dict[str, Union[str, int]]
    ) -> None:
        if self.devices.get(device_id) is None:
            raise HTTPException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(
            f"Setting fixes speed for device: {device_id} with args: {speed_kwargs}"
        )
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_fixed_speed({speed_kwargs}) "
            )
            status_job = self.device_executor.submit(
                device_id, lc_device.set_fixed_speed, **speed_kwargs
            )
            status_job.result(
                timeout=DEVICE_TIMEOUT_SECS
            )  # maximum timeout for setting data on the device
        except BaseException as err:
            log.error("Error setting fixed speed:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def set_speed_profile(self, device_id: int, speed_kwargs: Dict[str, Any]) -> None:
        if self.devices.get(device_id) is None:
            raise HTTPException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(
            f"Setting speed profile for device: {device_id} with args: {speed_kwargs}"
        )
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_speed_profile({speed_kwargs}) "
            )
            status_job = self.device_executor.submit(
                device_id, lc_device.set_speed_profile, **speed_kwargs
            )
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except BaseException as err:
            log.error("Error setting speed profile:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def set_color(self, device_id: int, color_kwargs: Dict[str, Any]) -> None:
        if self.devices.get(device_id) is None:
            raise HTTPException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Setting color for device: {device_id} with args: {color_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_color({color_kwargs}) "
            )
            status_job = self.device_executor.submit(
                device_id, lc_device.set_color, **color_kwargs
            )
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except BaseException as err:
            log.error("Error setting color:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def set_screen(self, device_id: int, screen_kwargs: Dict[str, str]) -> None:
        if self.devices.get(device_id) is None:
            raise HTTPException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Setting screen for device: {device_id} with args: {screen_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug_lc(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_screen({screen_kwargs}) "
            )
            start_screen_update = time.time()
            status_job = self.device_executor.submit(
                device_id, lc_device.set_screen, **screen_kwargs
            )
            # after setting the screen, sometimes an immediate status request comes back with 0, so we wait a small amount
            wait_job = self.device_executor.submit(device_id, lambda: time.sleep(0.01))
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
            wait_job.result(timeout=DEVICE_TIMEOUT_SECS)
            log.debug(
                f"Time taken to update the screen for device: {device_id}, including waiting on other commands: "
                f"{time.time() - start_screen_update}"
            )
        except BaseException as err:
            log.error("Error setting screen:", exc_info=err)
            raise LiquidctlException("Unexpected Device communication error") from err

    def disconnect_all(self) -> None:
        for device_id, lc_device in self.devices.items():
            self._disconnect_device(device_id, lc_device)
        self.devices.clear()

    def _disconnect_device(self, device_id: int, lc_device: BaseDriver) -> None:
        log.debug_lc(f"LC #{device_id} {lc_device.__class__.__name__}.disconnect() ")
        disconnect_job = self.device_executor.submit(device_id, lc_device.disconnect)
        disconnect_job.result(timeout=DEVICE_TIMEOUT_SECS)

    def shutdown(self) -> None:
        for device_id, lc_device in self.devices.items():
            if isinstance(
                lc_device, CorsairHidPsu
            ):  # attempt to reset fan control back to hardware
                log.debug_lc(
                    f"LC #{device_id} {lc_device.__class__.__name__}.initialize() "
                )
                init_job = self.device_executor.submit(device_id, lc_device.initialize)
                init_job.result(timeout=DEVICE_TIMEOUT_SECS)
        self.disconnect_all()
        self.device_executor.shutdown()

    @staticmethod
    def _stringify_status(
        statuses: List[Tuple[str, Union[str, int, float], str]]
    ) -> Statuses:
        return [(str(status[0]), str(status[1]), str(status[2])) for status in statuses]
