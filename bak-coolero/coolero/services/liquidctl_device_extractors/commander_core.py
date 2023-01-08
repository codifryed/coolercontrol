#  Coolero - monitor and control your cooling and other devices
#  Copyright (c) 2021  Guy Boldon
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
from typing import List, Dict, Any

from liquidctl.driver import commander_core
from liquidctl.driver.commander_core import CommanderCore

from coolero.models.channel_info import ChannelInfo
from coolero.models.device_info import DeviceInfo
from coolero.models.lighting_mode import LightingMode
from coolero.models.speed_options import SpeedOptions
from coolero.models.status import TempStatus, ChannelStatus
from coolero.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class CommanderCoreExtractor(LiquidctlDeviceInfoExtractor):
    supported_driver = CommanderCore
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_temp = 20
    _max_temp = 100

    @classmethod
    def extract_info(cls, device_instance: CommanderCore) -> DeviceInfo:
        if device_instance._has_pump:
            cls._channels['pump'] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=20,  # current liquidctl implementation allows for 0%, is that right?
                    max_duty=100,
                    profiles_enabled=False,  # may change soon
                    fixed_enabled=True,
                    manual_profiles_enabled=True  # remove if above changes
                )
            )
        fan_channel_names = [f'fan{i + 1}' for i in range(commander_core._FAN_COUNT)]
        for channel_name in fan_channel_names:
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=0,
                    max_duty=100,
                    profiles_enabled=False,  # may change soon
                    fixed_enabled=True,
                    manual_profiles_enabled=True  # remove if above changes
                )
            )

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
            temp_min=cls._min_temp,
            temp_max=cls._max_temp,
            temp_ext_available=True
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, channel_name: str) -> List[LightingMode]:
        return []

    @classmethod
    def _get_temperatures(cls, status_dict: Dict[str, Any], device_id: int) -> List[TempStatus]:
        temps = []
        water = cls._get_water_temp(status_dict)
        if water is not None:
            temps.append(TempStatus('water', water, 'Water', f'LC#{device_id} Water'))
        probes = cls._get_temp_probes(status_dict)
        temps.extend([
            TempStatus(name, temp, name.capitalize(), f'LC#{device_id} {name.capitalize()}')
            for name, temp in probes
        ])
        return temps

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        channel_statuses: List[ChannelStatus] = []
        pump_rpm = cls._get_pump_rpm(status_dict)
        if pump_rpm is not None:
            channel_statuses.append(ChannelStatus('pump', rpm=pump_rpm))
        multiple_fans_rpm = cls._get_multiple_fans_rpm_2(status_dict)
        channel_statuses.extend(
            ChannelStatus(name, rpm=rpm)
            for name, rpm in multiple_fans_rpm
        )
        return channel_statuses
