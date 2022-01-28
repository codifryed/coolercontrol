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

from liquidctl.driver import commander_pro
from liquidctl.driver.commander_pro import CommanderPro

from models.channel_info import ChannelInfo
from models.device_info import DeviceInfo
from models.lighting_mode import LightingMode
from models.speed_options import SpeedOptions
from models.status import TempStatus, ChannelStatus
from services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class CommanderProExtractor(LiquidctlDeviceInfoExtractor):
    supported_driver = CommanderPro
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_temp = 20
    _max_temp = 60  # Check driver for more info. Basically >60 is considered unsafe, so we use the safe default max

    @classmethod
    def extract_info(cls, device_instance: CommanderPro) -> DeviceInfo:
        for channel_name in device_instance._fan_names:
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=0,
                    max_duty=100,
                    profiles_enabled=True,
                    fixed_enabled=True
                )
            )

        for channel_name in device_instance._led_names:
            cls._channels[channel_name] = ChannelInfo(
                lighting_modes=cls._get_filtered_color_channel_modes(channel_name)
            )

        cls._lighting_speeds = ['slow', 'medium', 'fast']

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
            temp_min=cls._min_temp,
            temp_max=cls._max_temp,
            profile_max_length=6
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, channel_name: str) -> List[LightingMode]:
        channel_modes = []
        for mode_name, _ in commander_pro._MODES.items():
            channel_modes.append(LightingMode(mode_name, 1, 1, False, False))
        return channel_modes

    @classmethod
    def _get_temperatures(cls, status_dict: Dict[str, Any], device_id: int) -> List[TempStatus]:
        probes = cls._get_temp_probes(status_dict)
        return [
            TempStatus(name, temp, name.capitalize(), f'#{device_id} {name.capitalize()}')
            for name, temp in probes
        ]

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        channel_statuses: List[ChannelStatus] = []
        multiple_fans_rpm = cls._get_multiple_fans_rpm(status_dict)
        for name, rpm in multiple_fans_rpm:
            channel_statuses.append(ChannelStatus(name, rpm=rpm))
        return channel_statuses
