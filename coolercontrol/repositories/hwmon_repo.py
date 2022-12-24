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

import contextlib
import glob
import logging
import os
import re
from collections import Counter
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from time import sleep
from typing import List, Pattern, Tuple, Dict

import matplotlib
import numpy

from coolercontrol.models.channel_info import ChannelInfo
from coolercontrol.models.device import Device, DeviceType
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.settings import Setting
from coolercontrol.models.speed_options import SpeedOptions
from coolercontrol.models.status import Status, ChannelStatus, TempStatus
from coolercontrol.repositories.cpu_repo import PSUTIL_CPU_SENSOR_NAMES
from coolercontrol.repositories.devices_repository import DevicesRepository
from coolercontrol.repositories.hwmon_daemon_client import HwmonDaemonClient
from coolercontrol.services.shell_commander import ShellCommander
from coolercontrol.settings import Settings, UserSettings

_LOG = logging.getLogger(__name__)
_GLOB_PWM_PATH: str = '/sys/class/hwmon/hwmon*/pwm*'
_GLOB_PWM_PATH_CENTOS: str = '/sys/class/hwmon/hwmon*/device/pwm*'  # CentOS has an intermediate /device directory:
_PATTERN_PWN_PATH_NUMBER: Pattern = re.compile(r'.*/pwm\d+$')
_PATTERN_PWN_FILE: Pattern = re.compile(r'^pwm\d+$')
_GLOB_TEMP_PATH: str = '/sys/class/hwmon/hwmon*/temp*_input'
_GLOB_TEMP_PATH_CENTOS: str = '/sys/class/hwmon/hwmon*/device/temp*_input'
_PATTERN_TEMP_FILE: Pattern = re.compile(r'^temp\d+_input$')
_PATTERN_HWMON_PATH_NUMBER: Pattern = re.compile(r'/hwmon\d+')
_PATTERN_NUMBER: Pattern = re.compile(r'\d+')
_PWM_ENABLE_MANUAL: str = '1'
_PWM_ENABLE_AUTOMATIC_DEFAULT: int = 2
_DRIVER_NAMES_ALREADY_USED_BY_LIQUIDCTL: List[str] = ['nzxtsmart2', 'kraken3', 'kraken2', 'smartdevice']
_LAPTOP_DRIVER_NAMES: List[str] = ['thinkpad', 'asus-nb-wmi', 'asus_fan']


class HwmonChannelType(str, Enum):
    FAN = 'fan'
    TEMP = 'temp'

    def __str__(self) -> str:
        return str.__str__(self)


@dataclass
class HwmonChannelInfo:
    type: HwmonChannelType
    number: int
    pwm_enable_default: int | None = field(compare=False, default=None)
    name: str | None = field(compare=False, default=None)
    pwm_mode_supported: bool = field(compare=False, default=False)


@dataclass
class HwmonDriverInfo:
    name: str
    path: Path
    model: str | None = field(compare=False, default=None)
    channels: List[HwmonChannelInfo] = field(default_factory=list, compare=False)


class HwmonRepo(DevicesRepository):
    """Repo for Hwmon system"""

    _hwmon_devices: Dict[int, Tuple[Device, HwmonDriverInfo]] = {}

    def __init__(self, devices: List[Device]) -> None:
        self._all_devices = devices  # perhaps useful for future info
        self._hwmon_daemon: HwmonDaemonClient | None = None
        self._hwmon_temps_enabled: bool = Settings.user.value(
            UserSettings.ENABLE_HWMON_TEMPS, defaultValue=False, type=bool)
        self.read_only: bool = False
        self._init_daemon_connection()
        super().__init__()
        _LOG.info('Successfully initialized')
        _LOG.debug('Initialized with status: %s', self._hwmon_devices)

    def _init_daemon_connection(self) -> None:
        self._hwmon_daemon = self._attempt_connection(session_daemon=True)
        if self._hwmon_daemon is not None:
            _LOG.info('Successfully connected to Session Daemon')
            return
        self._hwmon_daemon = self._attempt_connection(session_daemon=False)
        if self._hwmon_daemon is not None:
            _LOG.info('Successfully connected to System Daemon')
            return
        if Settings.user.value(UserSettings.ENABLE_HWMON, defaultValue=False, type=bool):
            if _ := ShellCommander.start_session_daemon():
                sleep(0.3)  # to allow the session daemon to fully startup before trying to connect
                self._hwmon_daemon = self._attempt_connection(session_daemon=True)
                if self._hwmon_daemon is not None:
                    _LOG.info('Successfully connected to Session Daemon')
                    return
            _LOG.error('Failed to create and establish connection with the daemon')
        self.read_only = True
        _LOG.info("Hwmon in Read-Only Mode")

    @staticmethod
    def _attempt_connection(session_daemon: bool) -> HwmonDaemonClient | None:
        try:
            return HwmonDaemonClient(session_daemon)
        except (OSError, ValueError):
            daemon_type: str = 'session' if session_daemon else 'system'
            _LOG.debug('Unable to establish connection with %s daemon', daemon_type)
        return None

    @property
    def statuses(self) -> List[Device]:
        return [device for device, _ in self._hwmon_devices.values()]

    def update_statuses(self) -> None:
        for device, driver in self._hwmon_devices.values():
            device.status = Status(
                temps=self._extract_temp_statuses(device.type_id, driver),
                channels=self._extract_fan_statuses(driver)
            )
            _LOG.debug('HWMON device: %s status was updated with: %s', device.name, device.status)

    def shutdown(self) -> None:
        if self._hwmon_daemon is not None:
            for _, driver_info in self._hwmon_devices.values():
                self._reset_pwm_enable_to_default(driver_info)
            if self._hwmon_daemon.is_session_daemon:
                self._hwmon_daemon.shutdown()  # not as convenient but we have no other way to shut it down otherwise
            else:
                self._hwmon_daemon.close_connection()
        self._hwmon_devices.clear()
        _LOG.debug("Hwmon Repo shutdown")

    def set_settings(self, hwmon_device_id: int, setting: Setting) -> str | None:
        _, driver = self._hwmon_devices[hwmon_device_id]
        if self._hwmon_daemon is not None:
            try:
                if setting.speed_fixed is not None:
                    successful: bool = self._set_fixed_speed(
                        driver, setting.channel_name, setting.speed_fixed, setting.pwm_mode
                    )
                    if successful:
                        return driver.name
                    _LOG.error("Failure trying to apply hwmon settings. See daemon log")
                    return "ERROR Setting not applied"
                elif setting.speed_profile:
                    _LOG.error('Speed Profiles are not supported for HWMON devices')
                elif setting.lighting is not None:
                    _LOG.error('Lighting Settings are not supported for HWMON devices')
                return None
            except (IOError, OSError) as ex:
                _LOG.error('An Error has occurred when trying to set the settings: %s', ex)
                permissions_error = 'Permission denied' in str(ex)
                return 'ERROR Permission denied' if permissions_error else None
        else:
            _LOG.warning('Setting hwmon speed was attempted without a running coolercontrold daemon')
            return 'ERROR daemon not running'

    def set_channel_to_default(self, hwmon_device_id: int, setting: Setting) -> str | None:
        if self._hwmon_daemon is None:
            return 'ERROR daemon not running'
        _, driver = self._hwmon_devices[hwmon_device_id]
        channel: HwmonChannelInfo | None = self._find_fan_channel(driver, setting.channel_name)
        if channel is None:
            _LOG.error("Unable to find channel with name: %s for device: %s", setting.channel_name, driver.name)
            return "ERROR unknown channel"
        self._set_pwm_mode(channel.number, driver, setting)
        if channel.pwm_enable_default is None:
            return driver.name
        try:
            pwm_path = driver.path.joinpath(f'pwm{channel.number}_enable')
            current_pwm_enable = int(pwm_path.read_text().strip())
            if current_pwm_enable != channel.pwm_enable_default:
                successful: bool = self._hwmon_daemon.apply_setting(pwm_path, str(channel.pwm_enable_default))
                if successful:
                    _LOG.info(
                        'Device: %s Channel: %s pwm_enable has been set to original value of: %s',
                        driver.name, channel.number, channel.pwm_enable_default
                    )
                    return driver.name
                else:
                    _LOG.error('pwm_enable has not been reset in a reasonable amount of time')
                    return 'ERROR coolercontrold communication error'
            else:
                _LOG.info(
                    'Device: %s Channel: %s pwm_enable already set to original value of: %s',
                    driver.name, channel.number, channel.pwm_enable_default
                )
                return driver.name
        except (IOError, OSError) as err:
            _LOG.error(
                'Something went wrong with device: %s '
                'trying to set the original pwm%s_enable setting to its original value of: %s',
                driver.name,
                channel.number,
                channel.pwm_enable_default,
                exc_info=err
            )
            return 'ERROR applying hwmon settings'

    def _set_pwm_mode(self, channel_number: int, driver: HwmonDriverInfo, setting: Setting) -> None:
        if setting.pwm_mode is None:
            return
        try:
            self._hwmon_daemon.apply_setting(
                driver.path.joinpath(f'pwm{channel_number}_mode'), str(setting.pwm_mode)
            )
        except (IOError, OSError) as err:
            _LOG.error("Error setting pwm_mode to: %s", setting.pwm_mode, exc_info=err)

    def _reset_pwm_enable_to_default(self, driver: HwmonDriverInfo) -> None:
        """This returns all the channel pwm_enable settings back to the original setting from startup"""
        for channel in driver.channels:
            if channel.type != HwmonChannelType.FAN or channel.pwm_enable_default is None:
                continue
            try:
                pwm_path = driver.path.joinpath(f'pwm{channel.number}_enable')
                current_pwm_enable = int(pwm_path.read_text().strip())
                if current_pwm_enable != channel.pwm_enable_default:
                    self._hwmon_daemon.apply_setting(pwm_path, str(channel.pwm_enable_default))
            except (IOError, OSError) as err:
                _LOG.error(
                    'Something went wrong with device: %s '
                    'trying to set the original pwm%s_enable setting to its original value of: %s',
                    driver.name,
                    channel.number,
                    channel.pwm_enable_default,
                    exc_info=err
                )

    def _initialize_devices(self) -> None:
        base_paths: List[Path] = self._find_all_hwmon_device_paths()
        if not base_paths:
            _LOG.warning('No HWMon devices were found, try running sensors-detect')
            return
        hwmon_drivers_unsorted: List[HwmonDriverInfo] = []
        for base_path in base_paths:
            driver_name = self._get_driver_name(base_path)
            if self._is_already_used_by_liquidctl(driver_name):
                continue
            fans = self._initialize_fans(base_path, driver_name)
            temps = self._initialize_temps(base_path, driver_name)
            model = self._get_device_model_name(base_path)
            channels = fans + temps
            hwmon_driver_info = HwmonDriverInfo(driver_name, base_path, model, channels)
            hwmon_drivers_unsorted.append(hwmon_driver_info)
        self._remove_devices_without_data(hwmon_drivers_unsorted)
        self._handle_duplicate_device_names(hwmon_drivers_unsorted)
        # resorted by name to help maintain some semblance of order after reboots & device changes
        hwmon_drivers: List[HwmonDriverInfo] = sorted(hwmon_drivers_unsorted, key=lambda dev: dev.name)
        _LOG.debug('HWMON device drivers found: %s', hwmon_drivers)
        self._map_to_our_device_model(hwmon_drivers)
        self._update_device_colors()

    @staticmethod
    def _find_all_hwmon_device_paths() -> List[Path]:
        """
        Get distinct sorted hwmon paths that have either fan controls or temps.
        Due to issues with CentOS, this is the easiest way to verify said paths are correct
        """
        pwm_base_names: List[str] = glob.glob(_GLOB_PWM_PATH)
        if not pwm_base_names:
            pwm_base_names = glob.glob(_GLOB_PWM_PATH_CENTOS)
        all_base_path_names: List[Path] = [
            Path(path).resolve().parent
            for path in pwm_base_names
            # search for only pwm\d+ files (no _mode, _enable, etc):
            if _PATTERN_PWN_PATH_NUMBER.match(path)
        ]
        temp_base_names: List[str] = glob.glob(_GLOB_TEMP_PATH)
        if not temp_base_names:
            temp_base_names = glob.glob(_GLOB_TEMP_PATH_CENTOS)
        temp_base_path_names: List[Path] = [
            Path(path).resolve().parent
            for path in temp_base_names
        ]
        all_base_path_names.extend(temp_base_path_names)
        return sorted(set(all_base_path_names))

    @staticmethod
    def _get_driver_name(base_path: Path) -> str:
        try:
            return base_path.joinpath('name').read_text().strip()
        except (IOError, OSError):  # lots can go wrong here, staying safe
            _LOG.warning('Hwmon driver at location:%s has no name set, using default', base_path)
        hwmon_str = _PATTERN_HWMON_PATH_NUMBER.search(str(base_path)).group()
        hwmon_number = _PATTERN_NUMBER.search(hwmon_str, len(hwmon_str) - 2).group()
        return hwmon_number

    @staticmethod
    def _is_already_used_by_liquidctl(driver_name: str) -> bool:
        """
        Here we currently will hide HWMON devices that are primarily used by liquidctl.
        There aren't that many at the moment so this is currently the easiest way.
        Liquidctl offers more features, like RGB control, that hwmon doesn't offer yet.
        """
        return driver_name in _DRIVER_NAMES_ALREADY_USED_BY_LIQUIDCTL

    def _initialize_fans(self, base_path: Path, driver_name: str) -> list[HwmonChannelInfo]:
        fans: List[HwmonChannelInfo] = []
        dir_listing: List[str] = os.listdir(str(base_path))  # returns an empty list on error
        for dir_entry in dir_listing:
            if _PATTERN_PWN_FILE.match(dir_entry):
                channel_number: int = int(_PATTERN_NUMBER.search(dir_entry, len(dir_entry) - 2).group())
                should_skip, current_pwm_enable = self._should_skip_fan(base_path, channel_number, driver_name)
                if should_skip:
                    continue
                pwm_enable_default: int | None = self._adjusted_pwm_default(current_pwm_enable, driver_name)
                channel_name = self._get_fan_channel_name(base_path, channel_number)
                pwm_mode_supported = self._determine_pwm_mode_support(base_path, channel_number)
                fans.append(
                    HwmonChannelInfo(
                        type=HwmonChannelType.FAN,
                        number=channel_number,
                        pwm_enable_default=pwm_enable_default,
                        name=channel_name,
                        pwm_mode_supported=pwm_mode_supported
                    )
                )
        fans = sorted(fans, key=lambda ch: ch.number)
        self._handle_duplicate_channel_names(fans)
        _LOG.debug('HWMON pwm fans detected: %s for %s', fans, base_path)
        return fans

    @staticmethod
    def _should_skip_fan(base_path: Path, channel_number: int, driver_name: str) -> Tuple[bool, int | None]:
        """
        Not all drivers apparently have pwm_enable for their fans. In that case there is no "automatic" mode available
        pwm_enable setting options:
        - 0 : full speed / off (not used/recommended)
        - 1 : manual control (setting pwm* will adjust fan speed)
        - 2 : automatic (primarily used by on-board/chip fan control, like laptops or mobos without smart fan control)
        - 3 : "Fan Speed Cruise" mode (?)
        - 4 : "Smart Fan III" mode (NCT6775F only)
        - 5 : "Smart Fan IV" mode (modern MoBo's with build-in smart fan control probably use this)
        """
        reasonable_filter_enabled: bool = Settings.user.value(
            UserSettings.ENABLE_HWMON_FILTER, defaultValue=True, type=bool
        )
        try:
            current_pwm_enable = int(base_path.joinpath(f'pwm{channel_number}_enable').read_text().strip())
        except (IOError, OSError):
            current_pwm_enable: int | None = None
            _LOG.warning(f"No pwm_enable found for fan#{channel_number}")
        try:
            if reasonable_filter_enabled and current_pwm_enable == 0:
                # a value of 0 (off) can mean there's no fan connected for some devices,
                # it would be unexpected if this was the default setting
                return True, _PWM_ENABLE_AUTOMATIC_DEFAULT
            pwm_value = int(base_path.joinpath(f'pwm{channel_number}').read_text().strip())
            fan_rpm = int(base_path.joinpath(f'fan{channel_number}_input').read_text().strip())
            if reasonable_filter_enabled and fan_rpm == 0 and pwm_value > 255 * 0.25 \
                    and driver_name not in _LAPTOP_DRIVER_NAMES:
                # laptops on startup are running 0 rpm with sometimes high pwm_value
                # if no fan rpm but power is substantial, probably not connected
                #  (some fans need more than a little power to start spinning)
                return True, current_pwm_enable
            return False, current_pwm_enable
        except (IOError, OSError) as err:
            _LOG.warning('Error reading fan status: %s', err)
            return True, None

    @staticmethod
    def _adjusted_pwm_default(current_pwm_enable: int | None, driver_name: str) -> int | None:
        """
        Some drivers like thinkpad should have an automatic fallback for safety reasons, regardless of the current value
        """
        if current_pwm_enable is None:
            return None
        return 2 if driver_name in _LAPTOP_DRIVER_NAMES else current_pwm_enable

    @staticmethod
    def _get_fan_channel_name(base_path: Path, channel_number: int) -> str:
        try:
            if label := base_path.joinpath(f'fan{channel_number}_label').read_text().strip():
                return label
            else:
                _LOG.debug('Fan label is empty for fan #%s from %s', channel_number, base_path)
        except (IOError, OSError):
            _LOG.debug('Fan label not found for fan #%s from %s', channel_number, base_path)
        return f'fan{channel_number}'

    def _determine_pwm_mode_support(self, base_path: Path, channel_number: int) -> bool:
        """we then need to verify that setting this option is indeed supported (per pwm channel)"""
        if self._hwmon_daemon is None:
            return False
        #  0 = DC mode, 1 = PWM Mode. Not every device may have this option.
        try:
            fan_pwm_mode: int | None = int(
                base_path.joinpath(f'pwm{channel_number}_mode').read_text().strip())
            dc_mode_supported: bool = self._hwmon_daemon.apply_setting(
                base_path.joinpath(f'pwm{channel_number}_mode'), '0'
            )
            pwm_mode_supported: bool = self._hwmon_daemon.apply_setting(
                base_path.joinpath(f'pwm{channel_number}_mode'), '1'
            )
            self._hwmon_daemon.apply_setting(
                base_path.joinpath(f'pwm{channel_number}_mode'), str(fan_pwm_mode)
            )
            if dc_mode_supported and pwm_mode_supported:
                return True
        except (IOError, OSError):
            _LOG.debug('PWM Mode not found for fan #%s from %s', channel_number, base_path)
        return False

    def _initialize_temps(self, base_path: Path, driver_name: str) -> list[HwmonChannelInfo]:
        temps: List[HwmonChannelInfo] = []
        if not self._hwmon_temps_enabled or self._should_skip_already_used_temps(driver_name):
            return temps
        dir_listing: List[str] = os.listdir(str(base_path))  # returns an empty list on error
        for dir_entry in dir_listing:
            if _PATTERN_TEMP_FILE.match(dir_entry):
                channel_number: int = int(_PATTERN_NUMBER.search(dir_entry, len(dir_entry) - 8).group())
                if self._should_skip_temp(base_path, channel_number):
                    continue
                channel_name = self._get_temp_channel_name(base_path, channel_number)
                temps.append(
                    HwmonChannelInfo(
                        type=HwmonChannelType.TEMP,
                        number=channel_number,
                        name=channel_name
                    )
                )
        temps = self._remove_unreasonable_temps(temps)
        temps = sorted(temps, key=lambda ch: ch.number)
        self._handle_duplicate_channel_names(temps)
        _LOG.debug('HWMON temps detected: %s for %s', temps, base_path)
        return temps

    @staticmethod
    def _should_skip_already_used_temps(driver_name: str) -> bool:
        """
        This is mainly used to remove cpu temps, as we already have methods for that that use hwmon by default
        """
        devices_to_hide: List[str] = list(PSUTIL_CPU_SENSOR_NAMES)
        if not Settings.user.value(UserSettings.ENABLE_HWMON_FILTER, defaultValue=True, type=bool):
            # display all thinkpad temps even though the important ones are covered by other hwmon drivers
            devices_to_hide.remove('thinkpad')
        return driver_name in devices_to_hide

    @staticmethod
    def _should_skip_temp(base_path: Path, channel_number: int) -> bool:
        try:
            # temp readings come in thousandths by default, i.e. 35.0C == 35000
            temp_value: float = float(base_path.joinpath(f'temp{channel_number}_input').read_text().strip()) / 1000.0
            # these values are considered sane for a connected temp sensor
            return temp_value <= 0.0 or temp_value > 100
        except (IOError, OSError) as err:
            _LOG.warning('Error reading temp status: %s', err)
            return True

    @staticmethod
    def _get_temp_channel_name(base_path: Path, channel_number: int) -> str:
        try:
            if label := base_path.joinpath(f'temp{channel_number}_label').read_text().strip():
                return label
            else:
                _LOG.debug('Temp label is empty for temp #%s from %s', channel_number, base_path)
        except (IOError, OSError):
            _LOG.debug('Temp label not found for temp #%s from %s', channel_number, base_path)
        return f'temp{channel_number}'

    @staticmethod
    def _remove_unreasonable_temps(temps: List[HwmonChannelInfo]) -> List[HwmonChannelInfo]:
        if not Settings.user.value(UserSettings.ENABLE_HWMON_FILTER, defaultValue=True, type=bool):
            return temps
        # this removes other temps when 'Composite' is present.
        for temp in temps:
            if temp.name == 'Composite':
                return [temp]
        return temps

    @staticmethod
    def _get_device_model_name(base_path: Path) -> str | None:
        try:
            if model := base_path.joinpath('device/model').read_text().strip():
                return model
        except (IOError, OSError):
            _LOG.debug('Temp label not found from %s', base_path)
        return None

    @staticmethod
    def _remove_devices_without_data(hwmon_drivers: List[HwmonDriverInfo]) -> None:
        for index in reversed(range(len(hwmon_drivers))):
            if not hwmon_drivers[index].channels:
                del hwmon_drivers[index]

    def _handle_duplicate_device_names(self, hwmon_drivers: List[HwmonDriverInfo]) -> None:
        """check if there are duplicate device names but different device paths and adjust i.e. nvme drivers"""
        # our custom counter is not the most efficient but works well for our quite small lists
        duplicate_name_count: Dict[int, int] = {}
        for sd_index, starting_driver in enumerate(hwmon_drivers):
            duplicate_name_count[sd_index] = sum(
                sd_index == other_index or starting_driver.name == other_driver.name
                for other_index, other_driver in enumerate(hwmon_drivers)
            )
        for driver_index, count in duplicate_name_count.items():
            if count > 1:
                driver = hwmon_drivers[driver_index]
                driver.name = self._get_alternative_device_name(driver)

    @staticmethod
    def _handle_duplicate_channel_names(channels: List[HwmonChannelInfo]) -> None:
        """
        Check for duplicated channel names from hwmon labels and add numbers in case
        This is a regression from using liquidctl as the base for setting Settings
        (channel name is always unique in liquidctl, but not necessarily in other systems)
        """
        duplicate_name_count: Counter = Counter(
            [channel.name for channel in channels]
        )
        for name, count in duplicate_name_count.items():
            if count > 1:
                name_count: int = 0
                for channel in channels:
                    if channel.name == name:
                        name_count += 1
                        channel.name = f"{channel.name} #{name_count}"

    @staticmethod
    def _get_alternative_device_name(driver: HwmonDriverInfo) -> str:
        """Search for best alternative name to use in case of duplicate device name"""
        with contextlib.suppress(IOError, OSError):
            alternatives: Dict[str, str] = {}
            for line in driver.path.joinpath('device/uevent').read_text().splitlines():
                lines = line.split('=')
                if len(lines) != 2:
                    continue
                alternatives[lines[0].strip()] = lines[1].strip()
            dev_name = alternatives.get('DEVNAME')
            if dev_name is not None:
                return dev_name
            minor_num = alternatives.get('MINOR')
            if minor_num is not None:
                return driver.name + minor_num
        if driver.model is not None:
            return driver.model
        return driver.name

    def _map_to_our_device_model(self, hwmon_drivers: List[HwmonDriverInfo]) -> None:
        for index, driver in enumerate(hwmon_drivers):
            device_info = DeviceInfo(
                channels={
                    channel.name: ChannelInfo(speed_options=SpeedOptions(
                        fixed_enabled=True,
                        profiles_enabled=False,
                        manual_profiles_enabled=True
                    ))
                    for channel in driver.channels
                    if channel.type == HwmonChannelType.FAN  # Temps are not channel controls (terminology issue)
                },
                temp_min=0,
                temp_max=100,
                temp_ext_available=True,
                profile_max_length=21,
                model=driver.model
            )
            device_id = index + 1
            status = Status(
                channels=self._extract_fan_statuses(driver),
                temps=self._extract_temp_statuses(device_id, driver)
            )
            device = Device(
                _name=driver.name,
                _type_id=(DeviceType.HWMON, device_id),
                _status_current=status,
                _info=device_info
            )
            self._hwmon_devices[device_id] = (device, driver)

    @staticmethod
    def _extract_fan_statuses(driver: HwmonDriverInfo) -> List[ChannelStatus]:
        channels: List[ChannelStatus] = []
        for channel in driver.channels:
            if channel.type != HwmonChannelType.FAN:
                continue
            try:
                fan_rpm: int = int(driver.path.joinpath(f'fan{channel.number}_input').read_text().strip())
                fan_duty: int = round(int(driver.path.joinpath(f'pwm{channel.number}').read_text().strip()) / 2.55)
            except (IOError, OSError):
                fan_rpm = 0
                fan_duty = 0
            fan_pwm_mode: int | None = None
            if channel.pwm_mode_supported:
                with contextlib.suppress(IOError, OSError):
                    fan_pwm_mode = int(driver.path.joinpath(f'pwm{channel.number}_mode').read_text().strip())
            channels.append(
                ChannelStatus(
                    name=channel.name,
                    rpm=fan_rpm,
                    duty=fan_duty,
                    pwm_mode=fan_pwm_mode
                )
            )
        return channels

    def _extract_temp_statuses(self, device_id: int, driver: HwmonDriverInfo) -> List[TempStatus]:
        temps: List[TempStatus] = []
        if not self._hwmon_temps_enabled:
            return temps
        for channel in driver.channels:
            if channel.type != HwmonChannelType.TEMP:
                continue
            try:
                temp_value: float = float(
                    driver.path.joinpath(f'temp{channel.number}_input').read_text().strip()
                ) / 1000.0  # temp readings come in thousandths by default, i.e. 35.0C == 35000
            except (IOError, OSError):
                # mostly likely not found for a moment, but since we detected it on startup, should come back
                temp_value = 0.0
            temps.append(
                TempStatus(
                    name=channel.name,
                    temp=temp_value,
                    frontend_name=channel.name.capitalize(),
                    external_name=f'HW#{device_id} {channel.name.capitalize()}'
                )
            )
        return temps

    def _set_fixed_speed(self, driver: HwmonDriverInfo, channel_name: str, duty: int, pwm_mode: int | None) -> bool:
        pwm_value: str = str(int(self._clamp(duty, 0, 100) * 2.55))
        channel: HwmonChannelInfo | None = self._find_fan_channel(driver, channel_name)
        if channel is None:
            _LOG.error("Unable to find channel with name: %s for device: %s", channel_name, driver.name)
            return False
        if channel.pwm_enable_default is not None:
            pwm_path = driver.path.joinpath(f'pwm{channel.number}_enable')
            current_pwm_enable = pwm_path.read_text().strip()
            if current_pwm_enable != _PWM_ENABLE_MANUAL \
                    and not self._hwmon_daemon.apply_setting(pwm_path, _PWM_ENABLE_MANUAL):
                _LOG.error("Not able to enable manual fan control. Most likely because of a driver limitation.")
                return False
        if pwm_mode is not None:
            self._hwmon_daemon.apply_setting(driver.path.joinpath(f'pwm{channel.number}_mode'), str(pwm_mode))
        return self._hwmon_daemon.apply_setting(driver.path.joinpath(f'pwm{channel.number}'), pwm_value)

    @staticmethod
    def _clamp(value: int, clamp_min: int, clamp_max: int) -> int:
        return max(clamp_min, min(clamp_max, value))

    @staticmethod
    def _find_fan_channel(driver: HwmonDriverInfo, channel_name: str) -> HwmonChannelInfo | None:
        """channel_name will be the backend name, not the frontend name here. (conversion shouldn't be necessary)"""
        return next(
            (channel for channel in driver.channels
             if channel.type == HwmonChannelType.FAN and channel.name == channel_name),
            None
        )

    def _update_device_colors(self) -> None:
        number_of_colors: int = 0
        for device, _ in self._hwmon_devices.values():
            number_of_colors += len(device.status.temps)
            number_of_colors += len(device.status.channels)
        colors = self._create_all_colors(number_of_colors)
        color_counter: int = 0
        for device, _ in self._hwmon_devices.values():
            for temp_status in device.status.temps:
                device.colors[temp_status.name] = colors[color_counter]
                color_counter += 1
            for channel_status in device.status.channels:
                device.colors[channel_status.name] = colors[color_counter]
                color_counter += 1

    @staticmethod
    def _create_all_colors(number_of_colors: int) -> List[str]:
        if not number_of_colors:
            return []
        color_selectors = numpy.linspace(0.65, 1.0, number_of_colors)
        color_map = matplotlib.cm.get_cmap('winter')(color_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]
