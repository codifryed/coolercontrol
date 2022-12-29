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

import dataclasses
import logging
from operator import attrgetter

import matplotlib
import numpy
import requests
from dataclass_wizard import JSONWizard
from requests import Session

from coolercontrol.models.base_driver import BaseDriver
from coolercontrol.models.device import Device, DeviceType
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.settings import Setting
from coolercontrol.models.status import Status
from coolercontrol.repositories.devices_repository import DevicesRepository
from coolercontrol.settings import Settings, UserSettings

log = logging.getLogger(__name__)

TIMEOUT: float = 2.0
BASE_URL: str = "http://127.0.0.1:11987"
PATH_HANDSHAKE: str = "/handshake"
PATH_GET_DEVICES: str = "/devices"
PATH_GET_STATUS: str = "/status"

CPU_TEMP: str = "CPU Temp"


@dataclasses.dataclass
class LcInfo:
    driver_type: BaseDriver
    firmware_version: str | None
    unknown_asetek: bool


@dataclasses.dataclass
class DeviceDto:
    name: str
    type: DeviceType
    type_index: int
    uid: str
    lc_info: LcInfo | None
    info: DeviceInfo | None

    def to_device(self) -> Device:
        if self.lc_info is None:
            lc_driver_type = None
            firmware_version = None
        else:
            lc_driver_type = self.lc_info.driver_type
            firmware_version = self.lc_info.firmware_version

        return Device(
            _uid=self.uid,
            _name=self.name,
            _type_id=(self.type, self.type_index),
            _lc_driver_type=lc_driver_type,
            _lc_init_firmware_version=firmware_version,
            _info=self.info
        )


@dataclasses.dataclass
class DevicesResponse(JSONWizard):
    devices: list[DeviceDto]


@dataclasses.dataclass
class StatusDto:
    type: DeviceType
    type_index: int
    uid: str
    status_history: list[Status]


@dataclasses.dataclass
class StatusResponse(JSONWizard):
    devices: list[StatusDto]


class DaemonRepo(DevicesRepository):

    def __init__(self) -> None:
        self._devices: dict[str, Device] = {}
        self._client: Session = requests.Session()
        # self._excluded_temps: dict[str, str] = {}
        # self._excluded_channels: dict[str, str] = {}
        self._composite_temps_enabled: bool = Settings.user.value(UserSettings.ENABLE_COMPOSITE_TEMPS, defaultValue=False, type=bool)
        super().__init__()
        log.info('CoolerControl Daemon Repo Successfully initialized')
        log.debug('Initialized with devices: %s', self._devices)

    @property
    def statuses(self) -> list[Device]:
        return list(self._devices.values())

    def update_statuses(self) -> None:
        if not len(self._devices):
            return
        try:
            response = self._client.post(BASE_URL + PATH_GET_STATUS, timeout=TIMEOUT, json={})
            assert response.ok
            status_response: StatusResponse = StatusResponse.from_json(response.text)
            for device in status_response.devices:
                if not len(device.status_history):
                    log.error("StatusResponse has an empty status_history.")
                    continue
                if device.type == DeviceType.COMPOSITE and not self._composite_temps_enabled:
                    continue
                current_status_update = device.status_history[0]
                corresponding_local_device = self._devices.get(device.uid)
                if corresponding_local_device is None:
                    log.warning("Device with UID: %s not found", device.uid)
                    continue  # can happen for ex. when changing settings before a restart
                last_status_in_history = corresponding_local_device.status
                if last_status_in_history.timestamp == current_status_update.timestamp:
                    log.warning("StatusResponse contains duplicate timestamp of already existing status")
                    break  # contains duplicates
                time_delta = (current_status_update.timestamp - last_status_in_history.timestamp)
                if time_delta.seconds > 2:  # 1 has an edge case where the update timing is on the edge and goes back and forth
                    self._fill_statuses(time_delta, last_status_in_history)
                    break  # loop done in _fill_statuses
                else:
                    self._devices[device.uid].status = current_status_update
        except BaseException as ex:
            log.error("Error updating device status", exc_info=ex)

    def shutdown(self) -> None:
        self._devices.clear()
        log.debug("CoolerControl Daemon Repo shutdown")

    def _initialize_devices(self) -> None:
        try:
            # handshake
            response = self._client.get(BASE_URL + PATH_HANDSHAKE)
            assert response.ok
            handshake_response: dict = response.json()
            assert handshake_response["shake"] is True

            # devices
            response = self._client.get(BASE_URL + PATH_GET_DEVICES, timeout=TIMEOUT)
            assert response.ok
            log.debug("Devices Response: %s", response.text)
            devices_response: DevicesResponse = DevicesResponse.from_json(response.text)
            devices_response.devices.sort(key=attrgetter("type", "type_index"))
            for device_dto in devices_response.devices:
                if device_dto.info is not None:
                    device_dto.info = DeviceInfo(
                        #  needed for sorting frozen model
                        channels=dict(sorted(device_dto.info.channels.items(), key=lambda x: x[0])),
                        lighting_speeds=device_dto.info.lighting_speeds,
                        temp_min=device_dto.info.temp_min,
                        temp_max=device_dto.info.temp_max,
                        temp_ext_available=device_dto.info.temp_ext_available,
                        profile_max_length=device_dto.info.profile_max_length,
                        profile_min_length=device_dto.info.profile_min_length,
                        model=device_dto.info.model
                    )
                self._devices[device_dto.uid] = device_dto.to_device()

            # status
            response = self._client.post(BASE_URL + PATH_GET_STATUS, timeout=TIMEOUT, json={"all": True})
            assert response.ok
            status_response: StatusResponse = StatusResponse.from_json(response.text)
            for device in status_response.devices:
                self._devices[device.uid].status_history = device.status_history
        except BaseException as ex:
            log.error("Error communicating with CoolerControl Daemon", exc_info=ex)
        if not self._composite_temps_enabled:
            # remove composite devices if not enabled
            for device in list(self._devices.values()):
                if device.type == DeviceType.COMPOSITE:
                    del self._devices[device.uid]
        # todo: filter reasonable sensors
        # todo: filter hwmon temps
        self._update_device_colors()

    def set_settings(self, hwmon_device_id: int, setting: Setting) -> str | None:
        return "driver.name" or "Error"

    def set_channel_to_default(self, hwmon_device_id: int, setting: Setting) -> str | None:
        return "driver.name" or "Error"

    def reinitialize_devices(self) -> None:
        """This is helpful/necessary after waking from sleep for example"""
        # todo: no longer needed -> remove call stack
        pass

    def _fill_statuses(self, time_delta, last_status_in_history):
        # for ex. this can happen after startup and after waking from sleep
        log.warning("There is a gap in statuses in the status_history of: %s Attempting to fill.", time_delta)
        response = self._client.post(BASE_URL + PATH_GET_STATUS, timeout=TIMEOUT,
                                     json={"since": str(last_status_in_history.timestamp)})
        assert response.ok
        status_response_since_last_status: StatusResponse = StatusResponse.from_json(response.text)
        for device in status_response_since_last_status.devices:
            if device.type == DeviceType.COMPOSITE and not self._composite_temps_enabled:
                continue
            self._devices[device.uid].status_history = device.status_history

    def _update_device_colors(self) -> None:
        self._update_cpu_device_colors()
        self._update_gpu_device_colors()
        self._update_normal_device_colors()
        self._update_composite_device_colors()

    def _update_cpu_device_colors(self) -> None:
        cpu_devices: list[Device] = [
            device
            for device in self._devices.values()
            if device.type == DeviceType.CPU
        ]
        number_of_colors: int = len(cpu_devices)  # for cpu one color per device is best
        colors = self._create_cpu_colors(number_of_colors)
        for i, device in enumerate(cpu_devices):
            for temp_status in device.status.temps:
                device.colors[temp_status.name] = colors[i]
            for channel_status in device.status.channels:
                device.colors[channel_status.name] = colors[i]

    @staticmethod
    def _create_cpu_colors(number_of_colors: int) -> list[str]:
        if not number_of_colors:
            return []
        colors_selectors = numpy.linspace(0.1, 0.35, number_of_colors)
        color_map = matplotlib.cm.get_cmap("autumn")(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]

    def _update_gpu_device_colors(self) -> None:
        gpu_devices: list[Device] = [
            device
            for device in self._devices.values()
            if device.type == DeviceType.GPU
        ]
        number_of_colors: int = 0
        for device in gpu_devices:
            number_of_colors += 1  # gpus usually only have temp, load, and one fan
            if len(device.status.channels) > 2:  # if by change there is more than load and one fan channel
                number_of_colors += (len(device.status.channels) - 2)
        colors = self._create_gpu_colors(number_of_colors)
        for i, device in enumerate(gpu_devices):
            for temp_status in device.status.temps:
                device.colors[temp_status.name] = colors[i]
            for ch_i, channel_status in enumerate(device.status.channels):
                channel_color_index = 0 if ch_i < 2 else ch_i - 1  # offset
                device.colors[channel_status.name] = colors[i + channel_color_index]

    @staticmethod
    def _create_gpu_colors(number_of_colors: int) -> list[str]:
        if not number_of_colors:
            return []
        colors_selectors = numpy.linspace(0, 1, number_of_colors)
        color_map = matplotlib.cm.get_cmap("Wistia")(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]

    def _update_normal_device_colors(self) -> None:
        all_other_devices: list[Device] = [
            device
            for device in self._devices.values()
            if device.type in [DeviceType.LIQUIDCTL, DeviceType.HWMON]
        ]
        number_of_colors: int = 0
        for device in all_other_devices:
            if len(device.status_history) == 0:
                continue  # ignore if there are no statuses
            number_of_colors += len(device.status.temps)
            number_of_colors += len(device.status.channels)
        colors = self._create_all_normal_colors(number_of_colors)
        color_counter: int = 0
        for device in all_other_devices:
            if len(device.status_history) == 0:
                continue  # ignore if there are no statuses
            for temp_status in device.status.temps:
                device.colors[temp_status.name] = colors[color_counter]
                color_counter += 1
            for channel_status in device.status.channels:
                device.colors[channel_status.name] = colors[color_counter]
                color_counter += 1

    @staticmethod
    def _create_all_normal_colors(number_of_colors: int) -> list[str]:
        if not number_of_colors:
            return []
        colors_selectors = numpy.linspace(0, 1, number_of_colors)
        color_map = matplotlib.cm.get_cmap("cool")(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]

    def _update_composite_device_colors(self) -> None:
        composite_devices: list[Device] = [
            device
            for device in self._devices.values()
            if device.type == DeviceType.COMPOSITE
        ]
        number_of_colors: int = 0
        for device in composite_devices:
            if len(device.status_history) == 0:
                continue  # ignore if there are no statuses
            number_of_colors += len(device.status.temps)
            number_of_colors += len(device.status.channels)
        colors = self._create_composite_colors(number_of_colors)
        color_counter: int = 0
        for device in composite_devices:
            if len(device.status_history) == 0:
                continue  # ignore if there are no statuses
            for temp_status in device.status.temps:
                device.colors[temp_status.name] = colors[color_counter]
                color_counter += 1
            for channel_status in device.status.channels:
                device.colors[channel_status.name] = colors[color_counter]
                color_counter += 1

    @staticmethod
    def _create_composite_colors(number_of_colors: int) -> list[str]:
        if not number_of_colors:
            return []
        colors_selectors = numpy.linspace(0.5, 0.9, number_of_colors)
        color_map = matplotlib.cm.get_cmap("copper")(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]
