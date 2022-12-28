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
GPU_FAN: str = "GPU Fan"


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
                if device.type == DeviceType.COMPOSITE \
                        and not Settings.user.value(UserSettings.ENABLE_COMPOSITE_TEMPS, defaultValue=False, type=bool):
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
                if time_delta.seconds > 1:  # 1 has an edge case where the call above has a different current timestamp than the following
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
            for device_dto in devices_response.devices:
                self._devices[device_dto.uid] = device_dto.to_device()

            # status
            response = self._client.post(BASE_URL + PATH_GET_STATUS, timeout=TIMEOUT, json={"all": True})
            assert response.ok
            status_response: StatusResponse = StatusResponse.from_json(response.text)
            for device in status_response.devices:
                self._devices[device.uid].status_history = device.status_history
        except BaseException as ex:
            log.error("Error communicating with CoolerControl Daemon", exc_info=ex)
        if not Settings.user.value(UserSettings.ENABLE_COMPOSITE_TEMPS, defaultValue=False, type=bool):
            for device in list(self._devices.values()):
                if device.type == DeviceType.COMPOSITE:
                    # remove composite devices if not enabled
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
            if device.type == DeviceType.COMPOSITE \
                    and not Settings.user.value(UserSettings.ENABLE_COMPOSITE_TEMPS, defaultValue=False, type=bool):
                continue
            self._devices[device.uid].status_history = device.status_history

    def _update_device_colors(self) -> None:
        # todo: update CPU, GPU, and Composite colors separately
        number_of_colors: int = 0
        for device in self._devices.values():
            if len(device.status_history) == 0:
                continue  # ignore if there are no statuses
            number_of_colors += len(device.status.temps)
            number_of_colors += len(device.status.channels)
        colors = self._create_all_colors(number_of_colors)
        color_counter: int = 0
        for device in self._devices.values():
            if len(device.status_history) == 0:
                continue  # ignore if there are no statuses
            for temp_status in device.status.temps:
                device.colors[temp_status.name] = colors[color_counter]
                color_counter += 1
            for channel_status in device.status.channels:
                device.colors[channel_status.name] = colors[color_counter]
                color_counter += 1

    @staticmethod
    def _create_all_colors(number_of_colors: int) -> list[str]:
        if not number_of_colors:
            return []
        colors_selectors = numpy.linspace(0, 1, number_of_colors)
        color_map = matplotlib.cm.get_cmap('cool')(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]
