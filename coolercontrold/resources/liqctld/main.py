#! /usr/bin/env python3

#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
#
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU General Public License as published by
#  the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU General Public License for more details.
#
#  You should have received a copy of the GNU General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.

import concurrent
import http
import importlib.metadata
import json
import logging
import logging as log
import os
import queue
import socket
import sys
import time
import traceback
from concurrent.futures import Future, ThreadPoolExecutor
from http import HTTPStatus
from typing import Any, Callable, Dict, List, Optional, Tuple, Union

import liquidctl

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
from liquidctl.driver.asetek import Legacy690Lc, Modern690Lc
from liquidctl.driver.base import BaseDriver
from liquidctl.driver.commander_pro import CommanderPro
from liquidctl.driver.corsair_hid_psu import CorsairHidPsu
from liquidctl.driver.hydro_platinum import HydroPlatinum
from liquidctl.driver.kraken2 import Kraken2
from liquidctl.driver.kraken3 import KrakenZ3
from liquidctl.driver.smart_device import SmartDevice, SmartDevice2

#####################################################################
# Basic Setup
#####################################################################

SOCKET_ADDRESS: str = "/run/coolercontrol-liqctld.sock"
DEVICE_TIMEOUT_SECS: float = 9.5
DEVICE_READ_STATUS_TIMEOUT_SECS: float = 0.550

#####################################################################
# Models
#####################################################################

Statuses = List[Tuple[str, str, str]]


class LiquidctlException(Exception):
    pass


class LiqctldException(Exception):
    def __init__(self, code: HTTPStatus, message: str) -> None:
        json_message: str = json.dumps(self._to_dict(code, message))
        super().__init__(json_message)
        self.message = json_message
        self.code = code

    @staticmethod
    def _to_dict(code: HTTPStatus, message: str) -> Dict[str, Any]:
        return {
            "code": code.value,
            "message": message,
        }

    def __str__(self) -> str:
        return self.message


class BaseModel:
    def to_dict(self) -> Dict[str, Any]:
        return self.__dict__

    def to_dict_no_none(self) -> Dict[str, Any]:
        """Returns a dictionary without any None values"""
        return {k: v for k, v in self.to_dict().items() if v is not None}

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Any:
        cls.__dict__.update(data)

    def to_json(self) -> str:
        return json.dumps(self.to_dict())


class DeviceProperties(BaseModel):
    def __init__(
        self,
        speed_channels=None,
        color_channels=None,
        supports_cooling: Optional[bool] = None,
        supports_cooling_profiles: Optional[bool] = None,
        supports_lighting: Optional[bool] = None,
        led_count: Optional[int] = None,
        lcd_resolution: Optional[Tuple[int, int]] = None,
    ):
        if color_channels is None:
            color_channels = list()
        if speed_channels is None:
            speed_channels = []
        self.speed_channels: List[str] = speed_channels
        self.color_channels: List[str] = color_channels
        self.supports_cooling: Optional[bool] = supports_cooling
        self.supports_cooling_profiles: Optional[bool] = supports_cooling_profiles
        self.supports_lighting: Optional[bool] = supports_lighting
        self.led_count: Optional[int] = led_count
        self.lcd_resolution: Optional[Tuple[int, int]] = lcd_resolution

    def to_dict(self) -> Dict[str, Any]:
        return {
            "speed_channels": self.speed_channels,
            "color_channels": self.color_channels,
            "supports_cooling": self.supports_cooling,
            "supports_cooling_profiles": self.supports_cooling_profiles,
            "supports_lighting": self.supports_lighting,
            "led_count": self.led_count,
            "lcd_resolution": self.lcd_resolution,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "DeviceProperties":
        return cls(
            speed_channels=data.get("speed_channels", []),
            color_channels=data.get("color_channels", []),
            supports_cooling=data.get("supports_cooling", None),
            supports_cooling_profiles=data.get("supports_cooling_profiles", None),
            supports_lighting=data.get("supports_lighting", None),
            led_count=data.get("led_count", None),
            lcd_resolution=data.get("lcd_resolution", None),
        )


class Device(BaseModel):
    def __init__(
        self,
        id: int,
        description: str,
        device_type: str,
        properties: DeviceProperties,
        liquidctl_version: str,
        serial_number: Optional[str] = None,
        hid_address: Optional[str] = None,
        hwmon_address: Optional[str] = None,
    ):
        self.id: int = id
        self.description: str = description
        self.device_type: str = device_type
        self.properties: DeviceProperties = properties
        self.liquidctl_version: str = liquidctl_version
        self.serial_number: Optional[str] = serial_number
        self.hid_address: Optional[str] = hid_address
        self.hwmon_address: Optional[str] = hwmon_address

    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "description": self.description,
            "device_type": self.device_type,
            "serial_number": self.serial_number,
            "properties": self.properties.to_dict(),
            "liquidctl_version": self.liquidctl_version,
            "hid_address": self.hid_address,
            "hwmon_address": self.hwmon_address,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Device":
        try:
            return cls(
                id=data["id"],
                description=data["description"],
                device_type=data["device_type"],
                properties=DeviceProperties.from_dict(data["properties"]),
                liquidctl_version=data["liquidctl_version"],
                serial_number=data.get("serial_number", None),
                hid_address=data.get("hid_address", None),
                hwmon_address=data.get("hwmon_address", None),
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid Device Body: {data}"
            ) from None


class Handshake(BaseModel):
    def __init__(self, shake: bool = False):
        self.shake: bool = shake

    def to_dict(self) -> Dict[str, Any]:
        return {
            "shake": self.shake,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Handshake":
        return cls(
            shake=data.get("shake", False),
        )


class InitRequest(BaseModel):
    def __init__(self, pump_mode: Optional[str] = None):
        self.pump_mode: Optional[str] = pump_mode

    def to_dict(self) -> Dict[str, Any]:
        return {
            "pump_mode": self.pump_mode,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "InitRequest":
        return cls(
            pump_mode=data.get("pump_mode", None),
        )


class FixedSpeedRequest(BaseModel):
    def __init__(self, channel: str, duty: int):
        self.channel: str = channel
        self.duty: int = duty

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "duty": self.duty,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "FixedSpeedRequest":
        try:
            return cls(
                channel=data["channel"],
                duty=data["duty"],
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid FixedSpeedRequest Body: {data}"
            ) from None


class SpeedProfileRequest(BaseModel):
    def __init__(
        self, channel: str, profile=None, temperature_sensor: Optional[int] = None
    ):
        self.channel: str = channel
        if profile is None:
            profile = []
        self.profile: List[Tuple[float, int]] = profile
        self.temperature_sensor: Optional[int] = temperature_sensor

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "profile": self.profile,
            "temperature_sensor": self.temperature_sensor,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "SpeedProfileRequest":
        try:
            model = cls(
                channel=data["channel"],
                profile=data.get("profile", []),
                temperature_sensor=data.get("temperature_sensor", None),
            )
            # Pydantic used to auto cast floats sent by the daemon to int.
            # This is still wanted because several liquidctl drivers require int as temps.
            # Also, the default liquidctl CLI operation uses int for temps,
            #  so it is consistent with the daemon and the UI doesn't allow <1C intervals.
            # If one wants more precise control, the user should use a Standard Function to avoid
            # use of the in-built speed profiles.
            model.profile = [
                (int(temp_duty[0]), temp_duty[1]) for temp_duty in list(model.profile)
            ]
            return model
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid SpeedProfileRequest Body: {data}"
            ) from None


class ColorRequest(BaseModel):
    def __init__(
        self,
        channel: str,
        mode: str,
        colors: List[List[int]],
        time_per_color: Optional[int] = None,
        speed: Optional[str] = None,
        direction: Optional[str] = None,
    ):
        self.channel: str = channel
        self.mode: str = mode
        self.colors: List[List[int]] = colors
        self.time_per_color: Optional[int] = time_per_color
        self.speed: Optional[str] = speed
        self.direction: Optional[str] = direction

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "mode": self.mode,
            "colors": self.colors,
            "time_per_color": self.time_per_color,
            "speed": self.speed,
            "direction": self.direction,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ColorRequest":
        try:
            return cls(
                channel=data["channel"],
                mode=data["mode"],
                colors=data["colors"],
                time_per_color=data.get("time_per_color", None),
                speed=data.get("speed", None),
                direction=data.get("direction", None),
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid ColorRequest Body: {data}"
            ) from None


class ScreenRequest(BaseModel):
    channel: str
    mode: str
    value: Optional[str]

    def __init__(
        self,
        channel: str,
        mode: str,
        value: Optional[str] = None,
    ):
        self.channel: str = channel
        self.mode: str = mode
        self.value: Optional[str] = value

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "mode": self.mode,
            "value": self.value,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ScreenRequest":
        try:
            return cls(
                channel=data["channel"],
                mode=data["mode"],
                value=data.get("value", None),
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid ScreenRequest Body: {data}"
            ) from None


#####################################################################
# Executor
#####################################################################


class _DeviceJob:
    def __init__(self, future: Future, fn: Callable, **kwargs) -> None:
        self.future = future
        self.fn = fn
        self.kwargs = kwargs

    def run(self) -> None:
        if not self.future.set_running_or_notify_cancel():
            return
        try:
            result = self.fn(**self.kwargs)
        except BaseException as exc:
            self.future.set_exception(exc)
            # Break a reference cycle with the exception 'exc'
            self = None
        else:
            self.future.set_result(result)


def _queue_worker(dev_queue: queue.SimpleQueue) -> None:
    try:
        while True:
            device_job: _DeviceJob = dev_queue.get()
            if device_job is None:
                return
            device_job.run()
            del device_job
    except BaseException as exc:
        sys.stderr.write(f"Exception in worker: {exc}\n")


class DeviceExecutor:
    """
    Simultaneous communications per device result in mangled data,
    so we keep each device to its own job queue.
    We simultaneously use a Thread Pool to handle communication with separate devices.
    This enables us to talk in parallel to multiple devices,
    but keep communication for each device synchronous,
    which results in a pretty big speedup for people who have multiple devices.
    """

    def __init__(self) -> None:
        self._device_channels: Dict[int, queue.SimpleQueue] = {}
        self._thread_pool: ThreadPoolExecutor = None

    def set_number_of_devices(self, number_of_devices: int) -> None:
        if number_of_devices < 1:
            return  # don't set any workers if there are no devices
        self._thread_pool = ThreadPoolExecutor(max_workers=number_of_devices)
        for dev_id in range(1, number_of_devices + 1):
            dev_queue = queue.SimpleQueue()
            self._device_channels[dev_id] = dev_queue
            self._thread_pool.submit(_queue_worker, dev_queue)

    def submit(self, device_id: int, fn: Callable, **kwargs) -> Future:
        future = Future()
        device_job = _DeviceJob(future, fn, **kwargs)
        self._device_channels[device_id].put(device_job)
        return future

    def device_queue_empty(self, device_id: int) -> bool:
        return self._device_channels[device_id].empty()

    def shutdown(self) -> None:
        for channel in self._device_channels.values():
            channel.put(None)  # ends queue_worker loops
        if self._thread_pool is not None:
            self._thread_pool.shutdown()
        self._device_channels.clear()


#####################################################################
# Service
#####################################################################


def get_liquidctl_version() -> str:
    try:
        return importlib.metadata.version("liquidctl")
    except importlib.metadata.PackageNotFoundError:
        return getattr(liquidctl, "__version__", "unknown")


class DeviceService:
    """
    The Service which keeps track of devices and handles all communication
    """

    def __init__(self) -> None:
        """
        To enable synchronous & parallel device communication, we use our own DeviceExecutor
        """
        self.devices: Dict[int, BaseDriver] = {}
        # this can be used to set specific flags like legacy/type/special
        #  from settings in coolercontrold
        self.device_infos: Dict[int, Any] = {}
        self.device_executor: DeviceExecutor = DeviceExecutor()
        self.device_status_cache: Dict[int, Statuses] = {}
        self.liquidctl_version: str = get_liquidctl_version()

    ###########################################################################
    # Device Startup

    def get_devices(self) -> List[Device]:
        log.debug("Getting device list")
        # if we've already searched for devices, don't do so again, just retrieve device info
        if self.devices:
            devices = [
                Device(
                    id=index_id,
                    description=lc_device.description,
                    device_type=type(lc_device).__name__,
                    serial_number=lc_device.serial_number,
                    properties=self._get_device_properties(lc_device),
                    liquidctl_version=self.liquidctl_version,
                    hid_address=(str(lc_device.address) if lc_device.address else None),
                    hwmon_address=(
                        str(lc_device._hwmon.path) if lc_device._hwmon else None
                    ),
                )
                for index_id, lc_device in self.devices.items()
            ]
            return devices
        try:  # otherwise find devices
            log.debug("liquidctl.find_liquidctl_devices()")
            devices: List[Device] = []
            found_devices = list(liquidctl.find_liquidctl_devices())
            # sets the number of threads to be used per device:
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
                hwmon = getattr(lc_device, "_hwmon", None)
                devices.append(
                    Device(
                        id=index_id,
                        description=description,
                        device_type=type(lc_device).__name__,
                        serial_number=serial_number,
                        properties=self._get_device_properties(lc_device),
                        liquidctl_version=self.liquidctl_version,
                        hid_address=(
                            str(lc_device.address) if lc_device.address else None
                        ),
                        hwmon_address=str(hwmon.path) if hwmon else None,
                    )
                )
            device_names = [device.description for device in devices]
            log.info(f"Devices found: {device_names}")
            return devices
        except ValueError as ve:  # ValueError can happen when no devices were found
            log.debug(f"ValueError when trying to find all devices: {ve}")
            log.info("No Liquidctl devices detected")
            return []

    @staticmethod
    def _get_device_properties(lc_device: BaseDriver) -> DeviceProperties:
        """
        Get device instance attributes to determine the specific configuration for a given device.
        """
        speed_channels: List[str] = []
        color_channels: List[str] = []
        supports_cooling: Optional[bool] = None
        supports_cooling_profiles: Optional[bool] = None
        supports_lighting: Optional[bool] = None
        led_count: Optional[int] = None
        lcd_resolution: Optional[Tuple[int, int]] = None
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
        elif isinstance(lc_device, KrakenZ3):
            color_channel_dict = getattr(lc_device, "_color_channels", {})
            color_channels = list(color_channel_dict.keys())
            lcd_resolution = getattr(lc_device, "lcd_resolution", None)
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
            lcd_resolution=lcd_resolution,
        )

    def _connect_device(self, device_id: int, lc_device: BaseDriver) -> None:
        log.debug(f"LC #{device_id} {lc_device.__class__.__name__}.connect() ")
        # currently only smbus devices have options for connect()
        connect_job = self.device_executor.submit(device_id, lc_device.connect)
        try:
            connect_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except RuntimeError as err:
            if "already open" in str(err):
                log.warning(f"{lc_device.description} already connected")
            else:
                raise LiquidctlException(
                    "Unexpected Device Communication Error"
                ) from err

    ###########################################################################
    # Device Management

    def set_device_as_legacy690(self, device_id: int) -> Device:
        """
        Modern and Legacy Asetek 690Lc devices have the same device ID.
        We ask the user to verify which device is connected
        so that we can correctly communicate with the device.
        """
        if self.devices.get(device_id) is None:
            raise LiqctldException(
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
                liquidctl_version=self.liquidctl_version,
                hid_address=(str(lc_device.address) if lc_device.address else None),
                hwmon_address=None,
            )
        elif not isinstance(lc_device, Modern690Lc):
            message = f"Device #{device_id} is not applicable to be downgraded to a Legacy690Lc"
            log.warning(message)
            raise LiqctldException(HTTPStatus.EXPECTATION_FAILED, message)
        log.info(f"Setting device #{device_id} as legacy690")
        self._disconnect_device(device_id, lc_device)
        log.debug("Legacy690Lc.find_liquidctl_devices()")
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
            liquidctl_version=self.liquidctl_version,
            hid_address=(str(lc_device.address) if lc_device.address else None),
            hwmon_address=None,
        )

    def initialize_device(self, device_id: int, init_args: Dict[str, str]) -> Statuses:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
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
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.initialize({init_args}) "
            )
            init_job = self.device_executor.submit(
                device_id, lc_device.initialize, **init_args
            )
            lc_init_status: Union[
                List[Tuple[str, Union[str, int, float], str]],
                None,  # a few devices can return None on initialization like the Legacy690Lc
            ] = init_job.result(timeout=DEVICE_TIMEOUT_SECS)
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}initialize() "
                f"RESPONSE: {lc_init_status}"
            )
            return self._stringify_status(lc_init_status)
        except OSError as os_exc:
            # OSError, when a device was found and there's a permissions error
            log.error(f"Device Initialization Error - {traceback.format_exc()}")
            raise LiquidctlException(
                f"Unexpected Device Communication Error - {os_exc}"
            ) from os_exc

    @staticmethod
    def _stringify_status(
        statuses: Union[List[Tuple[str, Union[str, int, float], str]], None],
    ) -> Statuses:
        return (
            [(str(status[0]), str(status[1]), str(status[2])) for status in statuses]
            if statuses is not None
            else []
        )

    def get_status(self, device_id: int) -> Statuses:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Getting status for device: {device_id}")
        try:
            return self._get_current_or_cached_device_status(device_id)
        except BaseException as err:
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.error(
                    f"Liquidctl Error getting status for device "
                    f"#{device_id} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def _get_current_or_cached_device_status(self, device_id: int) -> Statuses:
        lc_device = self.devices[device_id]
        log.debug(f"LC #{device_id} {lc_device.__class__.__name__}.get_status() ")
        status_job = self.device_executor.submit(device_id, lc_device.get_status)
        try:
            status: List[Tuple[str, Union[str, int, float], str]] = status_job.result(
                timeout=DEVICE_READ_STATUS_TIMEOUT_SECS
            )
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.get_status() RESPONSE: {status}"
            )
            serialized_status = self._stringify_status(status)
            self.device_status_cache[device_id] = serialized_status
            return serialized_status
        except concurrent.futures.TimeoutError as te:
            log.debug(
                f"Timeout occurred while trying to get device status for LC #{device_id}. "
                f"Reusing last status if possible."
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
                    # return the currently cached status immediately and
                    #  let the async request above refresh the cache in the background
                    return cached_status
                # else rerun the status request with a very long timeout
                #  and wait for the output so that the cache fills up at least once
                try:
                    return async_status_job.result(timeout=DEVICE_TIMEOUT_SECS)
                except concurrent.futures.TimeoutError as te:
                    log.error(
                        f"Unknown issue with device communication "
                        f"and no Status Cache yet filled for device LC #{device_id}"
                    )
                    raise te
                finally:
                    async_status_job.cancel()
            # otherwise, this was a future timeout with a job still running in the queue
            if cached_status is None:
                log.error(f"No Status Cache yet filled for device LC #{device_id}")
                raise te
            return cached_status
        finally:
            status_job.cancel()

    def _long_async_status_request(self, dev_id: int) -> Statuses:
        """
        This function is used to get the status in a non-blocking way for devices
        that have extreme latency.
        """
        lc_device = self.devices[dev_id]
        log.debug(f"LC #{dev_id} {lc_device.__class__.__name__}.get_status() ")
        status = lc_device.get_status()
        log.debug(
            f"LC #{dev_id} {lc_device.__class__.__name__}.get_status() RESPONSE: {status}"
        )
        serialized_status = self._stringify_status(status)
        self.device_status_cache[dev_id] = serialized_status
        return serialized_status

    def set_fixed_speed(
        self, device_id: int, speed_kwargs: Dict[str, Union[str, int]]
    ) -> None:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(
            f"Setting fixes speed for device: {device_id} with args: {speed_kwargs}"
        )
        try:
            lc_device = self.devices[device_id]
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_fixed_speed({speed_kwargs}) "
            )
            status_job = self.device_executor.submit(
                device_id, lc_device.set_fixed_speed, **speed_kwargs
            )
            # maximum timeout for setting data on the device:
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except BaseException as err:
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.error(
                    f"Liquidctl Error setting fixed speed: "
                    f"{speed_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def set_speed_profile(self, device_id: int, speed_kwargs: Dict[str, Any]) -> None:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(
            f"Setting speed profile for device: {device_id} with args: {speed_kwargs}"
        )
        try:
            lc_device = self.devices[device_id]
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_speed_profile({speed_kwargs}) "
            )
            status_job = self.device_executor.submit(
                device_id, lc_device.set_speed_profile, **speed_kwargs
            )
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except BaseException as err:
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.error(
                    f"Liquidctl Error setting color with {speed_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def set_color(self, device_id: int, color_kwargs: Dict[str, Any]) -> None:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Setting color for device: {device_id} with args: {color_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_color({color_kwargs}) "
            )
            status_job = self.device_executor.submit(
                device_id, lc_device.set_color, **color_kwargs
            )
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except BaseException as err:
            # color changes are considered non-critical
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.warning(
                    f"Liquidctl Error setting color with {color_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def set_screen(self, device_id: int, screen_kwargs: Dict[str, str]) -> None:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Setting screen for device: {device_id} with args: {screen_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_screen({screen_kwargs}) "
            )
            start_screen_update = time.time()
            screen_job = self.device_executor.submit(
                device_id, lc_device.set_screen, **screen_kwargs
            )
            # after setting the screen, sometimes an immediate status request comes back with 0,
            #  so we wait a small amount
            wait_job = self.device_executor.submit(device_id, lambda: time.sleep(0.01))
            screen_job.result(timeout=DEVICE_TIMEOUT_SECS)
            wait_job.result(timeout=DEVICE_TIMEOUT_SECS)
            log.debug(
                f"Time taken to update the screen for device: {device_id}, "
                f"including waiting on other commands: {time.time() - start_screen_update}"
            )
        except BaseException as err:
            # screen changes are considered non-critical
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.warning(
                    f"Liquidctl Error setting screen with "
                    f"{screen_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    ###########################################################################
    # Device Shutdown

    def disconnect_all(self) -> None:
        for device_id, lc_device in self.devices.items():
            self._disconnect_device(device_id, lc_device)
        self.devices.clear()

    def _disconnect_device(self, device_id: int, lc_device: BaseDriver) -> None:
        log.debug(f"LC #{device_id} {lc_device.__class__.__name__}.disconnect() ")
        disconnect_job = self.device_executor.submit(device_id, lc_device.disconnect)
        disconnect_job.result(timeout=DEVICE_TIMEOUT_SECS)

    def shutdown(self) -> None:
        """
        Shutdown all devices and their workers.
        This includes calling initialize() to attempt to reset devices to their default state,
        then calling disconnect() to disconnect all devices.
        """
        for device_id, lc_device in self.devices.items():
            if isinstance(
                lc_device, CorsairHidPsu
            ):  # attempt to reset fan control back to hardware
                log.debug(
                    f"LC #{device_id} {lc_device.__class__.__name__}.initialize() "
                )
                init_job = self.device_executor.submit(device_id, lc_device.initialize)
                init_job.result(timeout=DEVICE_TIMEOUT_SECS)
        self.disconnect_all()
        self.device_executor.shutdown()


#####################################################################
# HTTP UDS Server
#####################################################################


class HTTPHandler(http.server.BaseHTTPRequestHandler):
    server_version = "BasicHTTP/1.0"
    protocol_version = "HTTP/1.1"

    def __init__(self, request, client_address, server):
        self.device_service: DeviceService = server.device_service
        super().__init__(request, client_address, server)

    # get("/handshake")
    def handshake(self):
        log.info("Exchanging handshake")
        self._send(HTTPStatus.OK, Handshake(shake=True).to_json())

    # post("/quit")
    def quit_server(self):
        log.info("Quit command received. Shutting down.")
        self._send(HTTPStatus.OK, json.dumps({}))
        self.server.shutdown()
        # this also handles socket close:
        self.server.server_close()
        self.device_service.shutdown()

    # get("/devices")
    def get_devices(self):
        devices: List[Device] = self.device_service.get_devices()
        self._send(
            HTTPStatus.OK, json.dumps({"devices": [d.to_dict() for d in devices]})
        )

    # put("/devices/{device_id}/legacy690")
    def set_device_as_legacy690(self, device_id: int):
        device: Device = self.device_service.set_device_as_legacy690(device_id)
        return device

    # post("/devices/{device_id}/initialize")
    def init_device(self, device_id: int, init_request: dict):
        init_args: dict = InitRequest.from_dict(init_request).to_dict_no_none()
        status_response: Statuses = self.device_service.initialize_device(
            device_id, init_args
        )
        self._send(HTTPStatus.OK, json.dumps({"status": status_response}))

    # get("/devices/{device_id}/status")
    def get_status(self, device_id: int):
        status_response: Statuses = self.device_service.get_status(device_id)
        self._send(HTTPStatus.OK, json.dumps({"status": status_response}))

    # put("/devices/{device_id}/speed/fixed")
    def set_fixed_speed(self, device_id: int, speed_request: dict):
        speed_kwargs = FixedSpeedRequest.from_dict(speed_request).to_dict_no_none()
        self.device_service.set_fixed_speed(device_id, speed_kwargs)
        # empty success response needed for systemd socket service to not error on 0 byte content
        self._send(HTTPStatus.OK, json.dumps({}))

    # put("/devices/{device_id}/speed/profile")
    def set_speed_profile(self, device_id: int, speed_request: dict):
        speed_kwargs = SpeedProfileRequest.from_dict(speed_request).to_dict_no_none()
        self.device_service.set_speed_profile(device_id, speed_kwargs)
        self._send(HTTPStatus.OK, json.dumps({}))

    # put("/devices/{device_id}/color")
    def set_color(self, device_id: int, color_request: dict):
        color_kwargs = ColorRequest.from_dict(color_request).to_dict_no_none()
        self.device_service.set_color(device_id, color_kwargs)
        self._send(HTTPStatus.OK, json.dumps({}))

    # put("/devices/{device_id}/screen")
    def set_screen(self, device_id: int, screen_request: dict):
        # need None value for liquid mode
        screen_kwargs = ScreenRequest.from_dict(screen_request).to_dict()
        self.device_service.set_screen(device_id, screen_kwargs)
        self._send(HTTPStatus.OK, json.dumps({}))

    def _route_get_requests(self, path: List[str]):
        if not path:
            raise LiqctldException(HTTPStatus.BAD_REQUEST, "Invalid path")
        elif len(path) == 1 and path[0] == "handshake":
            # get("/handshake")
            self.handshake()
        elif len(path) == 1 and path[0] == "devices":
            # get("/devices")
            self.get_devices()
        elif len(path) == 3 and path[0] == "devices" and path[2] == "status":
            # get("/devices/{device_id}/status")
            device_id = self._try_cast_int(path[1])
            self.get_status(device_id)
        else:
            raise LiqctldException(HTTPStatus.NOT_FOUND, f"Path: {path} Not Found")

    def _route_post_requests(self, path: List[str], request_body: dict):
        if not path:
            raise LiqctldException(HTTPStatus.BAD_REQUEST, "Invalid path")
        elif len(path) == 1 and path[0] == "quit":
            # post("/quit")
            self.quit_server()
        elif len(path) == 3 and path[0] == "devices" and path[2] == "initialize":
            # post("/devices/{device_id}/initialize")
            device_id = self._try_cast_int(path[1])
            self.init_device(device_id, request_body)
        else:
            raise LiqctldException(HTTPStatus.NOT_FOUND, f"Path: {path} Not Found")

    def _route_put_requests(self, path: List[str], request_body: dict):
        if not path:
            raise LiqctldException(HTTPStatus.BAD_REQUEST, "Invalid path")
        elif len(path) == 3 and path[0] == "devices" and path[2] == "legacy690":
            # put("/devices/{device_id}/legacy690")
            device_id = self._try_cast_int(path[1])
            self.set_device_as_legacy690(device_id)
        elif (
            len(path) == 4
            and path[0] == "devices"
            and path[2] == "speed"
            and path[3] == "fixed"
        ):
            # put("/devices/{device_id}/speed/fixed")
            device_id = self._try_cast_int(path[1])
            self.set_fixed_speed(device_id, request_body)
        elif (
            len(path) == 4
            and path[0] == "devices"
            and path[2] == "speed"
            and path[3] == "profile"
        ):
            # put("/devices/{device_id}/speed/profile")
            device_id = self._try_cast_int(path[1])
            self.set_speed_profile(device_id, request_body)
        elif len(path) == 3 and path[0] == "devices" and path[2] == "color":
            # put("/devices/{device_id}/color")
            device_id = self._try_cast_int(path[1])
            self.set_color(device_id, request_body)
        elif len(path) == 3 and path[0] == "devices" and path[2] == "screen":
            # put("/devices/{device_id}/screen")
            device_id = self._try_cast_int(path[1])
            self.set_screen(device_id, request_body)
        else:
            raise LiqctldException(HTTPStatus.NOT_FOUND, f"Path: {path} Not Found")

    def _send(self, status: HTTPStatus, reply: str) -> None:
        # avoid exception in server.py address_string()
        self.client_address = ("",)
        self.send_response(status.value)
        reply_bytes = reply.encode("utf-8")
        self.send_header("Content-type", "application/json")
        self.send_header("Content-Length", str(len(reply_bytes)))
        self.send_header("Connection", "keep-alive")
        self.end_headers()
        self.wfile.write(reply_bytes)

    def _parse_path(self) -> List[str]:
        return [x for x in self.path.strip().split("/") if x]

    @staticmethod
    def _try_cast_int(value: str) -> int:
        try:
            return int(value)
        except ValueError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid path. Expected Integer: {value}"
            ) from None

    def log_message(self, format, *args):
        # server request logs are disabled by default and will use our logger
        if self.server.log_level > logging.DEBUG:
            return
        # super().log_message(format, *args)
        message = format % args
        log.debug(
            "%s - - [%s] %s\n"
            % (
                self.address_string(),
                self.log_date_time_string(),
                message.translate(self._control_char_table),
            )
        )

    def do_GET(self):
        try:
            self._route_get_requests(self._parse_path())
        except LiquidctlException as e:
            self._send(HTTPStatus.BAD_GATEWAY, str(e))
        except LiqctldException as e:
            self._send(e.code, e.message)
        except Exception:
            self._send(HTTPStatus.INTERNAL_SERVER_ERROR, str(traceback.format_exc()))

    def do_POST(self):
        size = int(self.headers.get("Content-Length", 0))
        raw_body = self.rfile.read(size)
        request_body: dict = json.loads(raw_body) if raw_body else {}
        try:
            self._route_post_requests(self._parse_path(), request_body)
        except LiquidctlException as e:
            self._send(HTTPStatus.BAD_GATEWAY, str(e))
        except LiqctldException as e:
            self._send(e.code, e.message)
        except Exception:
            self._send(HTTPStatus.INTERNAL_SERVER_ERROR, str(traceback.format_exc()))

    def do_PUT(self):
        size = int(self.headers.get("Content-Length", 0))
        raw_body = self.rfile.read(size)
        request_body: dict = json.loads(raw_body) if raw_body else {}
        try:
            self._route_put_requests(self._parse_path(), request_body)
        except LiquidctlException as e:
            self._send(HTTPStatus.BAD_GATEWAY, str(e))
        except LiqctldException as e:
            self._send(e.code, e.message)
        except Exception:
            self._send(HTTPStatus.INTERNAL_SERVER_ERROR, str(traceback.format_exc()))


def server_run(device_service: DeviceService) -> None:
    try:
        os.remove(SOCKET_ADDRESS)
    except OSError:
        pass
    server = http.server.ThreadingHTTPServer(SOCKET_ADDRESS, HTTPHandler, False)
    server.log_level = log.getLogger().getEffectiveLevel()
    server.device_service = device_service
    server.socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.socket.bind(SOCKET_ADDRESS)
    # restrict access to the socket
    os.chmod(SOCKET_ADDRESS, 0o600)
    # creates a listener thread for each connection:
    queue_size = max(1, len(device_service.devices))
    server.socket.listen(queue_size)
    server.serve_forever()


#####################################################################
# Main
#####################################################################


def setup_logging() -> None:
    env_log_level: Optional[str] = (
        os.getenv("CC_LOG")
        if os.getenv("CC_LOG") is not None
        else os.getenv("COOLERCONTROL_LOG")
    )
    # default & "INFO" levels:
    log_level = logging.INFO
    liquidctl_level = logging.WARNING
    if env_log_level:
        if env_log_level.lower() == "debug" or env_log_level.lower() == "trace":
            log_level = logging.DEBUG
            liquidctl_level = logging.DEBUG
        elif env_log_level.lower() == "warn":
            log_level = logging.WARNING
            liquidctl_level = logging.WARNING
        elif env_log_level.lower() == "error":
            log_level = logging.ERROR
            liquidctl_level = logging.ERROR
    root_logger = logging.getLogger("root")
    root_logger.setLevel(log_level)
    liquidctl_logger = logging.getLogger("liquidctl")
    liquidctl_logger.setLevel(liquidctl_level)


def main() -> None:
    setup_logging()
    log.info("Initializing liquidctl service...")
    device_service = DeviceService()
    # We call liquidctl to find all devices, so that we can adjust the number of threads needed
    #  for parallel device communication.
    device_service.get_devices()

    server_run(device_service)
    log.info("Liqctld service shutdown complete.")


if __name__ == "__main__":
    main()
