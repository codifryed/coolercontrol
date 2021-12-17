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
from typing import List, Dict

from liquidctl.driver import kraken3
from liquidctl.driver.kraken3 import KrakenZ3

from models.channel_info import ChannelInfo
from models.device_info import DeviceInfo
from models.lighting_mode import LightingMode
from models.speed_options import SpeedOptions
from services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class KrakenZ3Extractor(LiquidctlDeviceInfoExtractor):
    supported_driver = KrakenZ3
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_liquid_temp = 20
    _max_liquid_temp = 60

    @classmethod
    def extract_info(cls, device_instance: KrakenZ3) -> DeviceInfo:
        for channel_name, (_, duty_min, duty_max) in kraken3._SPEED_CHANNELS_KRAKENZ.items():
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=duty_min,
                    max_duty=duty_max,
                    profiles_enabled=True,
                    fixed_enabled=True
                )
            )

        for channel_name in kraken3._COLOR_CHANNELS_KRAKENZ:
            cls._channels[channel_name] = ChannelInfo(
                lighting_modes=cls._get_filtered_color_channel_modes(channel_name)
            )

        cls._lighting_speeds = list(kraken3._ANIMATION_SPEEDS.keys())

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
            min_temp=cls._min_liquid_temp,
            max_temp=cls._max_liquid_temp
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, channel_name: str) -> List[LightingMode]:
        channel_modes = []
        for mode_name, (_, _, speed_scale, min_colors, max_colors) in kraken3._COLOR_MODES.items():
            if 'backwards' not in mode_name:  # remove deprecated modes
                # todo: direction needs to done by hand per mode
                channel_modes.append(LightingMode(mode_name, min_colors, max_colors, (speed_scale > 0), True))
        return channel_modes