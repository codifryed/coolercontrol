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
from collections import defaultdict
from datetime import timedelta
from operator import attrgetter
from typing import Optional

import matplotlib
import numpy
import requests
from dataclass_wizard import JSONWizard
from requests import Session

from coolercontrol.dialogs.legacy_690_dialog import Legacy690Dialog
from coolercontrol.exceptions.restart_needed import RestartNeeded
from coolercontrol.models.base_driver import BaseDriver
from coolercontrol.models.device import Device, DeviceType
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.settings import Setting, LightingSettings, LcdSettings
from coolercontrol.models.status import Status
from coolercontrol.models.temp_source import TempSource
from coolercontrol.repositories.devices_repository import DevicesRepository
from coolercontrol.services.settings_observer import SettingsObserver
from coolercontrol.settings import Settings, UserSettings

log = logging.getLogger(__name__)

TIMEOUT: float = 2.0
DAEMON_IP_ADDRESS: str = "127.0.0.1"
DAEMON_PORT: int = 11987
BASE_URL: str = f"http://{DAEMON_IP_ADDRESS}:{DAEMON_PORT}"
PATH_HANDSHAKE: str = "/handshake"
PATH_DEVICES: str = "/devices"
PATH_STATUS: str = "/status"
PATH_SETTINGS: str = "/settings"
PATH_ASETEK: str = "/asetek690"
PATH_SHUTDOWN: str = "/shutdown"

LAPTOP_DRIVER_NAMES: list[str] = ["thinkpad", "asus-nb-wmi", "asus_fan"]
COMPOSITE_TEMP_NAME: str = "Composite"
# possible scheduled update variance (<100ms) + all devices updated avg timespan (~80ms)
MAX_UPDATE_TIMESTAMP_VARIATION: timedelta = timedelta(milliseconds=200)


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


@dataclasses.dataclass
class DaemonSettingsDto(JSONWizard):
    class _(JSONWizard.Meta):
        key_transform_with_dump = "SNAKE"

    apply_on_boot: bool | None
    handle_dynamic_temps: bool | None
    startup_delay: int | None
    smoothing_level: int | None


@dataclasses.dataclass
class TempSourceDto(JSONWizard):
    class _(JSONWizard.Meta):
        key_transform_with_dump = "SNAKE"

    temp_name: str
    device_uid: str

    @classmethod
    def from_settings(cls, temp_source: TempSource | None) -> Optional['TempSourceDto']:
        if temp_source is None:
            return None
        # the temp_source.name can be either the frontend_name (same device) or the external_name (different device)
        # let's consolidate it for the daemon to the real "name" for simplicity
        temp_name: str | None = next(
            (
                temp_status.name
                for temp_status in temp_source.device.status.temps
                if temp_status.external_name == temp_source.name
                or temp_status.frontend_name == temp_source.name
            ),
            None,
        )
        assert temp_name
        return cls(
            temp_name=temp_name,
            device_uid=temp_source.device.uid,
        )


@dataclasses.dataclass
class DeviceSettingDto(JSONWizard):
    class _(JSONWizard.Meta):
        key_transform_with_dump = "SNAKE"

    channel_name: str
    speed_fixed: int | None
    speed_profile: list[tuple[int, int]] | None
    temp_source: TempSourceDto | None
    lighting: LightingSettings | None
    lcd: LcdSettings | None
    pwm_mode: int | None
    reset_to_default: bool | None

    @classmethod
    def from_settings(cls, setting: Setting) -> 'DeviceSettingDto':
        return cls(
            channel_name=setting.channel_name,
            speed_fixed=setting.speed_fixed,
            speed_profile=None if len(setting.speed_profile) == 0 else setting.speed_profile,
            temp_source=TempSourceDto.from_settings(setting.temp_source),
            lighting=setting.lighting,
            lcd=setting.lcd,
            pwm_mode=setting.pwm_mode,
            reset_to_default=setting.reset_to_default,
        )


class DaemonRepo(DevicesRepository):

    def __init__(self) -> None:
        self._settings_observer = SettingsObserver()
        self.devices: dict[str, Device] = {}
        self._client: Session = requests.Session()
        self._composite_temps_enabled: bool = Settings.user.value(UserSettings.ENABLE_COMPOSITE_TEMPS, defaultValue=False, type=bool)
        self._hwmon_temps_enabled: bool = Settings.user.value(UserSettings.ENABLE_HWMON_TEMPS, defaultValue=False, type=bool)
        self._hwmon_filter_enabled: bool = Settings.user.value(UserSettings.ENABLE_HWMON_FILTER, defaultValue=True, type=bool)
        self._excluded_channel_names: dict[str, list[str]] = defaultdict(list)
        super().__init__()
        self._sync_daemon_settings()
        self._settings_observer.connect_on_change(self._daemon_settings_changed)
        log.info('CoolerControl Daemon Repo Successfully initialized')
        log.debug('Initialized with devices: %s', self.devices)

    @property
    def statuses(self) -> list[Device]:
        return list(self.devices.values())

    def update_statuses(self) -> None:
        if not len(self.devices):
            return
        try:
            response = self._client.post(BASE_URL + PATH_STATUS, timeout=TIMEOUT, json={})
            if not response.ok:
                log.error("Error getting status from CoolerControl Daemon: %s %s", response.status_code, response.text)
            assert response.ok
            status_response: StatusResponse = StatusResponse.from_json(response.text)
            duplicate_status_logged: bool = False
            for device in status_response.devices:
                if not len(device.status_history):
                    log.error("StatusResponse has an empty status_history.")
                    continue
                if device.type == DeviceType.COMPOSITE and not self._composite_temps_enabled:
                    continue
                current_status_update = device.status_history[0]  # only the current status is returned by default
                if device.type == DeviceType.HWMON and not self._hwmon_temps_enabled:
                    current_status_update.temps.clear()
                corresponding_local_device = self.devices.get(device.uid)
                if corresponding_local_device is None:
                    log.warning("Device with UID: %s not found", device.uid)
                    continue  # can happen for ex. when changing some filters before a restart
                if device.type == DeviceType.HWMON and self._hwmon_filter_enabled:
                    for temp in list(current_status_update.temps):
                        if temp.name == COMPOSITE_TEMP_NAME:
                            current_status_update.temps.clear()
                            current_status_update.temps.append(temp)
                    for i, channel in reversed(list(enumerate(current_status_update.channels))):
                        if channel.name in self._excluded_channel_names[device.uid]:
                            current_status_update.channels.pop(i)
                latest_status_in_history = corresponding_local_device.status
                if latest_status_in_history.timestamp == current_status_update.timestamp:
                    if not duplicate_status_logged:
                        log.warning("StatusResponse contains duplicate timestamp of already existing status")
                        duplicate_status_logged = True
                    continue
                time_delta = current_status_update.timestamp - MAX_UPDATE_TIMESTAMP_VARIATION - latest_status_in_history.timestamp
                if time_delta.seconds > 1:
                    self._fill_statuses(time_delta, latest_status_in_history)
                    break  # device loop done in _fill_statuses
                else:
                    self.devices[device.uid].status = current_status_update
        except BaseException as ex:
            log.error("Error updating device status", exc_info=ex)

    def shutdown(self) -> None:
        self.devices.clear()
        log.debug("CoolerControl Daemon Repo shutdown")

    def _initialize_devices(self) -> None:
        try:
            # handshake
            response = self._client.get(BASE_URL + PATH_HANDSHAKE)
            if not response.ok:
                log.error("Error handshaking with CoolerControl Daemon: %s %s", response.status_code, response.text)
            assert response.ok
            handshake_response: dict = response.json()
            assert handshake_response["shake"] is True

            # devices
            response = self._client.get(BASE_URL + PATH_DEVICES, timeout=TIMEOUT)
            if not response.ok:
                log.error("Error getting devices from CoolerControl Daemon: %s %s", response.status_code, response.text)
            assert response.ok
            log.debug("Devices Response: %s", response.text)
            devices_response: DevicesResponse = DevicesResponse.from_json(response.text)
            devices_response.devices.sort(key=attrgetter("type", "type_index"))
            for device_dto in devices_response.devices:
                if device_dto.lc_info is not None and device_dto.lc_info.unknown_asetek:
                    self._request_if_device_is_legacy690(device_dto)
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
                self.devices[device_dto.uid] = device_dto.to_device()
            self._load_all_statuses()
        except RestartNeeded as ex:
            raise ex
        except BaseException as ex:
            log.error("Error communicating with CoolerControl Daemon", exc_info=ex)
        self._filter_devices()
        self._update_device_colors()

    def set_settings(self, device_uid: str, setting: Setting) -> str | None:
        try:
            device_name = self.devices[device_uid].name_short
            response = self._client.patch(f"{BASE_URL}{PATH_DEVICES}/{device_uid}{PATH_SETTINGS}", timeout=TIMEOUT,
                                          json=DeviceSettingDto.from_settings(setting).to_dict())
            if not response.ok:
                log.error("Error trying to send settings to CoolerControl Daemon: %s %s", response.status_code, response.text)
            assert response.ok
            return device_name
        except BaseException as ex:
            log.error("Error communicating with CoolerControl Daemon", exc_info=ex)
            return "Communication Error"

    def _fill_statuses(self, time_delta, last_status_in_history):
        # for ex. this can happen after startup and after waking from sleep
        log.warning("There is a gap in statuses in the status_history of: %s seconds Attempting to fill.", time_delta.seconds)
        response = self._client.post(BASE_URL + PATH_STATUS, timeout=TIMEOUT,
                                     json={"since": str(last_status_in_history.timestamp)})
        if not response.ok:
            log.error("Error getting statuses-since from CoolerControl Daemon: %s %s", response.status_code, response.text)
        assert response.ok
        status_response_since_last_status: StatusResponse = StatusResponse.from_json(response.text)
        for device_dto in status_response_since_last_status.devices:
            if device_dto.type == DeviceType.COMPOSITE and not self._composite_temps_enabled:
                continue
            if device_dto.type == DeviceType.HWMON and not self._hwmon_temps_enabled:
                for status in device_dto.status_history:
                    status.temps.clear()
            if device_dto.type == DeviceType.HWMON and self._hwmon_filter_enabled:
                for status in device_dto.status_history:
                    for temp in list(status.temps):
                        if temp.name == COMPOSITE_TEMP_NAME:
                            status.temps.clear()
                            status.temps.append(temp)
                    for i, channel in reversed(list(enumerate(status.channels))):
                        if channel.name in self._excluded_channel_names[device_dto.uid]:
                            status.channels.pop(i)
            self.devices[device_dto.uid].status_history = device_dto.status_history
            while (device_dto.status_history[-1].timestamp - self.devices[device_dto.uid].status_history[0].timestamp).seconds > 1860:
                # clear out any statuses that are older than 31 mins (the max). For ex. helps with waking from sleep situations
                self.devices[device_dto.uid].status_history.pop(0)

    def _load_all_statuses(self):
        # status
        response = self._client.post(BASE_URL + PATH_STATUS, timeout=TIMEOUT, json={"all": True})
        if not response.ok:
            log.error("Error getting all statuses from CoolerControl Daemon: %s %s", response.status_code, response.text)
        assert response.ok
        status_response: StatusResponse = StatusResponse.from_json(response.text)
        for device in status_response.devices:
            self.devices[device.uid].status_history.clear()
            self.devices[device.uid].status_history = device.status_history

    def _filter_devices(self) -> None:
        for device in list(self.devices.values()):
            if device.type == DeviceType.COMPOSITE and not self._composite_temps_enabled:
                # remove composite devices if not enabled
                del self.devices[device.uid]
            if device.type == DeviceType.HWMON and not self._hwmon_temps_enabled:
                # remove temps from hwmon status
                for status in device.status_history:
                    status.temps.clear()
            if device.type == DeviceType.HWMON and self._hwmon_filter_enabled:
                # filter out unwanted sensors - not-used, invalid, not-helpful, etc
                for status in device.status_history:
                    for temp in list(status.temps):
                        # this removes other temps when "Composite" is present (for ex. SSD temp sensors)
                        if temp.name == COMPOSITE_TEMP_NAME:
                            status.temps.clear()
                            status.temps.append(temp)
                    if device.name not in LAPTOP_DRIVER_NAMES:  # laptops on startup are running 0 rpm with sometimes high pwm_value
                        for i, channel in reversed(list(enumerate(status.channels))):
                            if channel.rpm is not None and channel.rpm == 0 and channel.duty > 25:
                                # if no fan rpm but power is substantial, probably not connected
                                #  (some fans need more than a little power to start spinning)
                                self._excluded_channel_names[device.uid].append(channel.name)
                                status.channels.pop(i)

    def _update_device_colors(self) -> None:
        self._update_cpu_device_colors()
        self._update_gpu_device_colors()
        self._update_normal_device_colors()
        self._update_composite_device_colors()

    def _update_cpu_device_colors(self) -> None:
        cpu_devices: list[Device] = [
            device
            for device in self.devices.values()
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
            for device in self.devices.values()
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
            for device in self.devices.values()
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
            for temp_status in sorted(device.status.temps, key=attrgetter("name")):
                device.colors[temp_status.name] = colors[color_counter]
                color_counter += 1
            for channel_status in sorted(device.status.channels, key=attrgetter("name")):
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
            for device in self.devices.values()
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
            for temp_status in sorted(device.status.temps, key=attrgetter("name")):
                device.colors[temp_status.name] = colors[color_counter]
                color_counter += 1
            for channel_status in sorted(device.status.channels, key=attrgetter("name")):
                device.colors[channel_status.name] = colors[color_counter]
                color_counter += 1

    @staticmethod
    def _create_composite_colors(number_of_colors: int) -> list[str]:
        if not number_of_colors:
            return []
        colors_selectors = numpy.linspace(0.5, 0.9, number_of_colors)
        color_map = matplotlib.cm.get_cmap("copper")(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]

    def _request_if_device_is_legacy690(self, device_dto: DeviceDto) -> None:
        is_legacy_690: bool = Legacy690Dialog(device_dto.type_index).ask()
        response = self._client.patch(
            f"{BASE_URL}{PATH_DEVICES}/{device_dto.uid}{PATH_ASETEK}",
            timeout=TIMEOUT,
            json={"is_legacy690": is_legacy_690},
        )
        if not response.ok:
            log.error("Error sending asetek status to CoolerControl Daemon: %s %s", response.status_code, response.text)
        assert response.ok
        if is_legacy_690:  # restart of daemons & gui required
            response = self._client.post(BASE_URL + PATH_SHUTDOWN, timeout=TIMEOUT, json={})
            if not response.ok:
                log.error("Error sending shutdown to CoolerControl Daemon: %s %s", response.status_code, response.text)
            assert response.ok
            raise RestartNeeded()

    def _sync_daemon_settings(self) -> None:
        try:
            response = self._client.get(BASE_URL + PATH_SETTINGS, timeout=TIMEOUT)
            if not response.ok:
                log.error("Error syncing settings with CoolerControl Daemon: %s %s", response.status_code, response.text)
            assert response.ok
            daemon_settings: DaemonSettingsDto = DaemonSettingsDto.from_json(response.text)
            if daemon_settings.apply_on_boot is not None:
                Settings.user.setValue(UserSettings.LOAD_APPLIED_AT_BOOT, daemon_settings.apply_on_boot)
            if daemon_settings.handle_dynamic_temps is not None:
                Settings.user.setValue(UserSettings.ENABLE_DYNAMIC_TEMP_HANDLING, daemon_settings.handle_dynamic_temps)
            if daemon_settings.startup_delay is not None:
                Settings.user.setValue(UserSettings.STARTUP_DELAY, daemon_settings.startup_delay)
            if daemon_settings.smoothing_level is not None:
                Settings.user.setValue(UserSettings.SMOOTHING_LEVEL, daemon_settings.smoothing_level)
            Settings.user.sync()
        except BaseException as ex:
            log.error("Error syncing settings with CoolerControl Daemon", exc_info=ex)

    def _daemon_settings_changed(self, setting_changed: str) -> None:
        log.debug("Syncing settings with CoolerControl Daemon")
        Settings.user.sync()
        apply_on_boot: bool = Settings.user.value(UserSettings.LOAD_APPLIED_AT_BOOT, defaultValue=True, type=bool)
        handle_dynamic_temps: bool = Settings.user.value(UserSettings.ENABLE_DYNAMIC_TEMP_HANDLING, defaultValue=True, type=bool)
        startup_delay: int = Settings.user.value(UserSettings.STARTUP_DELAY, defaultValue=0, type=int)
        smoothing_level: int = Settings.user.value(UserSettings.SMOOTHING_LEVEL, defaultValue=0, type=int)
        daemon_settings = DaemonSettingsDto(apply_on_boot, handle_dynamic_temps, startup_delay, smoothing_level)
        try:
            response = self._client.patch(BASE_URL + PATH_SETTINGS, timeout=TIMEOUT, json=daemon_settings.to_dict())
            if not response.ok:
                log.error("Error syncing settings with CoolerControl Daemon: %s %s", response.status_code, response.text)
            assert response.ok
            if setting_changed == UserSettings.SMOOTHING_LEVEL:
                self._load_all_statuses()
                self._filter_devices()
                self._settings_observer.clear_graph_history()
        except BaseException as ex:
            log.error("Error syncing settings with CoolerControl Daemon", exc_info=ex)
