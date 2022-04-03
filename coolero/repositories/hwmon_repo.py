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
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Pattern, Tuple, Dict, Set

from coolero.models.channel_info import ChannelInfo
from coolero.models.device import Device, DeviceType
from coolero.models.device_info import DeviceInfo
from coolero.models.speed_options import SpeedOptions
from coolero.models.status import Status, ChannelStatus
from coolero.repositories.devices_repository import DevicesRepository
from coolero.services.shell_commander import ShellCommander
from coolero.settings import Settings

_LOG = logging.getLogger(__name__)
_GLOB_PWM_PATH: str = '/sys/class/hwmon/hwmon*/pwm*'
_GLOB_PWM_PATH_CENTOS: str = '/sys/class/hwmon/hwmon*/device/pwm*'  # CentOS has an intermediate /device directory:
_PATTERN_PWN_PATH_NUMBER: Pattern = re.compile(r'.*/pwm\d+$')
_PATTERN_HWMON_PATH_NUMBER: Pattern = re.compile(r'/hwmon\d+/')
_PATTERN_NUMBER: Pattern = re.compile(r'\d+')
_DRIVER_NAME: str = 'name'
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

    _hwmon_devices: List[Tuple[Device, HwmonDriverInfo]] = []

    def __init__(self, devices: List[Device]) -> None:
        self._all_devices = devices  # perhaps useful for future info
        super().__init__()
        _LOG.info('Initialized with status: %s', self._hwmon_devices)

    @property
    def statuses(self) -> List[Device]:
        return [device for device, _ in self._hwmon_devices]

    def update_statuses(self) -> None:
        for device, driver in self._hwmon_devices:
            device.status = self._extract_status(driver)
            _LOG.debug('HWMON device: %s status was updated with: %s', device.name, device.status)

    def shutdown(self) -> None:
        for _, driver_info in self._hwmon_devices:
            self._reset_pwm_enable_to_default(driver_info)
        self._hwmon_devices.clear()
        _LOG.debug("Hwmon Repo shutdown")

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
            try:
                fan_rpm = int(base_path.joinpath(f'fan{channel_number}_input').read_text().strip())
                if fan_rpm == 0:  # if no fan rpm is being reported, skip it
                    continue
                current_pwm_enable = int(base_path.joinpath(f'pwm{channel_number}_enable').read_text().strip())
                if current_pwm_enable == 0:
                    continue  # a value of 0 (off) can mean there's no fan connected for some devices
            except (IOError, OSError):
                continue  # if there's any error getting the above info, skip
            hwmon_str = _PATTERN_HWMON_PATH_NUMBER.search(path).group()
            hwmon_number = _PATTERN_NUMBER.search(hwmon_str, len(hwmon_str) - 2).group()
            try:
                driver_name = base_path.joinpath(_DRIVER_NAME).read_text().strip()
            except (IOError, OSError):  # lots can go wrong here, staying safe
                _LOG.warning('Hwmon driver at location:%s has no name set, using default', base_path)
                driver_name = hwmon_number
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
            device = Device(
                _name=driver.name,
                _type_id=(DeviceType.HWMON, index + 1),
                _status_current=status,
                _colors=colors,
                _info=device_info
            )
            self._hwmon_devices.append((device, driver))

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

    @staticmethod
    def _reset_pwm_enable_to_default(driver: HwmonDriverInfo) -> None:
        """This returns all the channel pwm_enable settings back to the original setting from startup"""
        for channel in driver.channels:
            try:
                pwm_path = driver.path.joinpath(f'pwm{channel.number}_enable')
                current_pwm_enable = int(pwm_path.read_text().strip())
                if current_pwm_enable != channel.pwm_original_default:
                    pwm_path.write_text(str(channel.pwm_original_default))
            except (IOError, OSError) as err:
                _LOG.error(
                    'Something went wrong with device: %s '
                    'trying to set the original pwm%s_enable setting to its original value of: %s',
                    driver.name,
                    channel.number,
                    channel.pwm_original_default,
                    exc_info=err
                )
