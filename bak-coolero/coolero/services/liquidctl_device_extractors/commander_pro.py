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
from typing import List, Dict, Any, Tuple

from liquidctl.driver.commander_pro import CommanderPro

from coolero.models.channel_info import ChannelInfo
from coolero.models.device_info import DeviceInfo
from coolero.models.lighting_mode import LightingMode
from coolero.models.speed_options import SpeedOptions
from coolero.models.status import TempStatus, ChannelStatus
from coolero.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class CommanderProExtractor(LiquidctlDeviceInfoExtractor):
    supported_driver = CommanderPro
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_temp = 20
    _max_temp = 60  # Check driver for more info. Basically >60 is considered unsafe, so we use the safe default max
    _lighting_modes: List[Tuple[str, int, int, bool, bool]] = [
        # Mode Info:
        # name, min_colors, max_colors, speed_enabled, direction_enabled
        ('off', 0, 0, False, False),
        ('fixed', 1, 1, False, False),
        ('color_shift', 0, 2, True, True),
        ('color_pulse', 0, 2, True, True),
        ('color_wave', 0, 2, True, True),
        ('visor', 0, 2, True, True),
        ('blink', 0, 2, True, True),
        ('marquee', 0, 1, True, True),
        ('sequential', 0, 1, True, True),
        ('rainbow', 0, 0, True, True),
        ('rainbow2', 0, 0, True, True)
    ]

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
            profile_max_length=6,
            profile_min_length=2
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, channel_name: str) -> List[LightingMode]:
        channel_modes = []
        for mode_name, min_colors, max_colors, speed_enabled, direction_enabled in cls._lighting_modes:
            channel_modes.append(LightingMode(
                mode_name, cls._channel_to_frontend_name(mode_name),
                min_colors, max_colors,
                speed_enabled, direction_enabled
            ))
        return channel_modes

    @classmethod
    def _get_temperatures(cls, status_dict: Dict[str, Any], device_id: int) -> List[TempStatus]:
        probes = cls._get_temp_probes(status_dict)
        return [
            TempStatus(name, temp, name.capitalize(), f'LC#{device_id} {name.capitalize()}')
            for name, temp in probes
        ]

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        channel_statuses: List[ChannelStatus] = []
        multiple_fans_rpm = cls._get_multiple_fans_rpm(status_dict)
        channel_statuses.extend(
            ChannelStatus(name, rpm=rpm)
            for name, rpm in multiple_fans_rpm
        )
        return channel_statuses
