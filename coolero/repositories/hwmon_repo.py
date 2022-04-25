#  Coolero - monitor and control your cooling and other devices
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

import glob
import logging
import re
import uuid
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Pattern, Tuple, Dict, Set, Optional

from coolero.models.channel_info import ChannelInfo
from coolero.models.device import Device, DeviceType
from coolero.models.device_info import DeviceInfo
from coolero.models.settings import Setting
from coolero.models.speed_options import SpeedOptions
from coolero.models.status import Status, ChannelStatus
from coolero.repositories.devices_repository import DevicesRepository
from coolero.repositories.hwmon_daemon_client import HwmonDaemonClient
from coolero.services.shell_commander import ShellCommander
from coolero.settings import Settings, UserSettings

_LOG = logging.getLogger(__name__)
_GLOB_PWM_PATH: str = '/sys/class/hwmon/hwmon*/pwm*'
_GLOB_PWM_PATH_CENTOS: str = '/sys/class/hwmon/hwmon*/device/pwm*'  # CentOS has an intermediate /device directory:
_PATTERN_PWN_PATH_NUMBER: Pattern = re.compile(r'.*/pwm\d+$')
_PATTERN_HWMON_PATH_NUMBER: Pattern = re.compile(r'/hwmon\d+')
_PATTERN_NUMBER: Pattern = re.compile(r'\d+')
_DRIVER_NAME: str = 'name'
_PWM_ENABLE_MANUAL: str = '1'
_DRIVER_NAMES_ALREADY_USED_BY_LIQUIDCTL = ['nzxtsmart2', 'kraken3', 'kraken2', 'smartdevice']


@dataclass(frozen=True)
class HwmonChannelInfo:
    number: int
    pwm_original_default: int = field(compare=False)


@dataclass(frozen=True)
class HwmonDriverInfo:
    name: str
    path: Path
    channels: List[HwmonChannelInfo] = field(default_factory=list, compare=False)


class HwmonRepo(DevicesRepository):
    """Repo for Hwmon system"""

    _hwmon_devices: Dict[int, Tuple[Device, HwmonDriverInfo]] = {}

    def __init__(self, devices: List[Device]) -> None:
        self._all_devices = devices  # perhaps useful for future info
        self._hwmon_daemon: HwmonDaemonClient | None = None
        key: bytes = str(uuid.uuid4()).encode('UTF-8')
        successfully_started_daemon: bool = ShellCommander.start_daemon(key)
        super().__init__()
        if successfully_started_daemon:
            try:
                self._hwmon_daemon = HwmonDaemonClient(key)
            except ValueError as err:
                _LOG.error('Unable to establish connection with hwmon daemon', exc_info=err)
        _LOG.info('Initialized with status: %s', self._hwmon_devices)

    @property
    def statuses(self) -> List[Device]:
        return [device for device, _ in self._hwmon_devices.values()]

    def update_statuses(self) -> None:
        for device, driver in self._hwmon_devices.values():
            device.status = self._extract_status(driver)
            _LOG.debug('HWMON device: %s status was updated with: %s', device.name, device.status)

    def shutdown(self) -> None:
        for _, driver_info in self._hwmon_devices.values():
            self._reset_pwm_enable_to_default(driver_info)
        self._hwmon_devices.clear()
        if self._hwmon_daemon is not None:
            self._hwmon_daemon.shutdown()
        ShellCommander.remove_tmp_hwmon_daemon_script()
        _LOG.debug("Hwmon Repo shutdown")

    def set_settings(self, hwmon_device_id: int, setting: Setting) -> str | None:
        _, driver = self._hwmon_devices[hwmon_device_id]
        if self._hwmon_daemon is not None:
            try:
                if setting.speed_fixed is not None:
                    successful: bool = self._set_fixed_speed(driver, setting.channel_name, setting.speed_fixed)
                    return driver.name if successful else 'ERROR Setting not applied'
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
            return 'ERROR Hwmon Daemon not enabled'

    def set_channel_to_default(self, hwmon_device_id: int, setting: Setting) -> Optional[str]:
        if self._hwmon_daemon is not None:
            _, driver = self._hwmon_devices[hwmon_device_id]
            channel_number: int = int(_PATTERN_NUMBER.search(setting.channel_name).group())
            channel = next((channel for channel in driver.channels if channel.number == channel_number), None)
            if channel is None:
                _LOG.error('Invalid Hwmon Channel Number: %s for device: %s', channel_number, driver.name)
                return 'ERROR unknown channel number'
            try:
                pwm_path = driver.path.joinpath(f'pwm{channel.number}_enable')
                current_pwm_enable = int(pwm_path.read_text().strip())
                if current_pwm_enable != channel.pwm_original_default:
                    successful: bool = self._hwmon_daemon.apply_setting(pwm_path, str(channel.pwm_original_default))
                    if successful:
                        _LOG.info(
                            'Device: %s Channel: %s pwm_enable has been set to original value of: %s',
                            driver.name, channel_number, channel.pwm_original_default
                        )
                        return driver.name
                    else:
                        _LOG.error('pwm_enable has not been reset in a reasonable amount of time')
                        return 'ERROR Hwmon communication error'
                else:
                    _LOG.info(
                        'Device: %s Channel: %s pwm_enable already set to original value of: %s',
                        driver.name, channel_number, channel.pwm_original_default
                    )
                    return driver.name
            except (IOError, OSError) as err:
                _LOG.error(
                    'Something went wrong with device: %s '
                    'trying to set the original pwm%s_enable setting to its original value of: %s',
                    driver.name,
                    channel.number,
                    channel.pwm_original_default,
                    exc_info=err
                )
                return 'ERROR applying hwmon settings'
        else:
            return 'ERROR Hwmon Daemon not enabled'

    def _initialize_devices(self) -> None:
        if not ShellCommander.sensors_data_exists():
            return
        usable_fans: Dict[HwmonDriverInfo, Set[HwmonChannelInfo]] = defaultdict(set)
        pwm_base_path_names = glob.glob(_GLOB_PWM_PATH)
        if not pwm_base_path_names:
            pwm_base_path_names = glob.glob(_GLOB_PWM_PATH_CENTOS)
        pwm_base_path_names = sorted(set(pwm_base_path_names))
        # simplify glob search for only pwm\d+ files (no _mode, _enable, etc):
        pwm_base_path_names = [path for path in pwm_base_path_names if _PATTERN_PWN_PATH_NUMBER.match(path)]

        for path in pwm_base_path_names:
            base_path: Path = Path(path).resolve().parent
            channel_number: int = int(_PATTERN_NUMBER.search(path, len(path) - 2).group())
            should_skip, current_pwm_enable = self._should_skip_fan(base_path, channel_number)
            if should_skip:
                continue
            driver_name = self._get_driver_name(base_path)
            hwmon_driver_info = HwmonDriverInfo(driver_name, base_path)
            usable_fans[hwmon_driver_info].add(HwmonChannelInfo(channel_number, current_pwm_enable))

        for driver_info, channel_infos in usable_fans.items():
            channels = sorted(channel_infos, key=lambda ch: ch.number)
            driver_info.channels.extend(channels)
        # sorted by name to help maintain some semblance of order after reboots & device changes
        hwmon_drivers = sorted(usable_fans.keys(), key=lambda dev: dev.name)
        _LOG.debug('HWMON pwm fans found: %s', hwmon_drivers)

        for index, driver in enumerate(hwmon_drivers):
            if self._device_already_used_by_liquidctl(driver):
                continue
            status = self._extract_status(driver)
            device_info = DeviceInfo(
                channels={
                    f'fan{channel.number}': ChannelInfo(speed_options=SpeedOptions(
                        fixed_enabled=True,
                        profiles_enabled=False,
                        manual_profiles_enabled=True
                    ))
                    for channel in driver.channels
                }
            )
            colors = {f'fan{channel_info.number}': Settings.theme['app_color']['green'] for channel_info in
                      driver.channels}
            device_id = index + 1
            device = Device(
                _name=driver.name,
                _type_id=(DeviceType.HWMON, device_id),
                _status_current=status,
                _colors=colors,
                _info=device_info
            )
            self._hwmon_devices[device_id] = (device, driver)

    @staticmethod
    def _should_skip_fan(base_path: Path, channel_number: int) -> Tuple[bool, int]:
        reasonable_filter_enabled: bool = Settings.user.value(
            UserSettings.ENABLE_HWMON_CHANNEL_FILTER, defaultValue=True, type=bool
        )
        try:
            current_pwm_enable = int(base_path.joinpath(f'pwm{channel_number}_enable').read_text().strip())
            if reasonable_filter_enabled and current_pwm_enable == 0:
                # a value of 0 (off) can mean there's no fan connected for some devices
                return True, current_pwm_enable
            pwm_value = int(base_path.joinpath(f'pwm{channel_number}').read_text().strip())
            fan_rpm = int(base_path.joinpath(f'fan{channel_number}_input').read_text().strip())
            if reasonable_filter_enabled and fan_rpm == 0 and pwm_value > 255 * 0.25:
                # if no fan rpm but power is substantial, probably not connected
                return True, current_pwm_enable
            return False, current_pwm_enable
        except (IOError, OSError) as err:
            _LOG.warning('Error reading fan status: %s', err)
            return True, 0

    @staticmethod
    def _get_driver_name(base_path: Path) -> str:
        hwmon_str = _PATTERN_HWMON_PATH_NUMBER.search(str(base_path)).group()
        hwmon_number = _PATTERN_NUMBER.search(hwmon_str, len(hwmon_str) - 2).group()
        try:
            return base_path.joinpath(_DRIVER_NAME).read_text().strip()
        except (IOError, OSError):  # lots can go wrong here, staying safe
            _LOG.warning('Hwmon driver at location:%s has no name set, using default', base_path)
            return hwmon_number

    @staticmethod
    def _extract_status(driver: HwmonDriverInfo) -> Status:
        channels: List[ChannelStatus] = []
        for channel in driver.channels:
            try:
                fan_rpm = int(driver.path.joinpath(f'fan{channel.number}_input').read_text().strip())
                fan_duty = int(int(driver.path.joinpath(f'pwm{channel.number}').read_text().strip()) / 2.55)
            except (IOError, OSError):
                fan_rpm = 0
                fan_duty = 0
            channels.append(
                ChannelStatus(
                    name=f'fan{channel.number}',
                    rpm=fan_rpm,
                    duty=fan_duty
                )
            )
        return Status(channels=channels)

    @staticmethod
    def _device_already_used_by_liquidctl(driver: HwmonDriverInfo) -> bool:
        """
        Here we currently will hide HWMON devices that are primarily used by liquidctl.
        There aren't that many at the moment so this is currently the easiest way.
        Liquidctl offers more features, like RGB control, that hwmon doesn't offer yet.
        """
        return driver.name in _DRIVER_NAMES_ALREADY_USED_BY_LIQUIDCTL

    def _reset_pwm_enable_to_default(self, driver: HwmonDriverInfo) -> None:
        """This returns all the channel pwm_enable settings back to the original setting from startup"""
        for channel in driver.channels:
            try:
                pwm_path = driver.path.joinpath(f'pwm{channel.number}_enable')
                current_pwm_enable = int(pwm_path.read_text().strip())
                if current_pwm_enable != channel.pwm_original_default:
                    self._hwmon_daemon.apply_setting(pwm_path, str(channel.pwm_original_default))
            except (IOError, OSError) as err:
                _LOG.error(
                    'Something went wrong with device: %s '
                    'trying to set the original pwm%s_enable setting to its original value of: %s',
                    driver.name,
                    channel.number,
                    channel.pwm_original_default,
                    exc_info=err
                )

    def _set_fixed_speed(self, driver: HwmonDriverInfo, channel_name: str, duty: int) -> bool:
        pwm_value: str = str(int(self._clamp(duty, 0, 100) * 2.55))
        channel_number: str = _PATTERN_NUMBER.search(channel_name).group()
        pwm_path = driver.path.joinpath(f'pwm{channel_number}_enable')
        current_pwm_enable = pwm_path.read_text().strip()
        if current_pwm_enable != _PWM_ENABLE_MANUAL:
            self._hwmon_daemon.apply_setting(pwm_path, _PWM_ENABLE_MANUAL)
        return self._hwmon_daemon.apply_setting(driver.path.joinpath(f'pwm{channel_number}'), pwm_value)

    @staticmethod
    def _clamp(value: int, clamp_min: int, clamp_max: int) -> int:
        return max(clamp_min, min(clamp_max, value))
