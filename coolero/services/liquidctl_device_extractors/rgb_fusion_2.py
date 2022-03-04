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

from liquidctl.driver import rgb_fusion2
from liquidctl.driver.rgb_fusion2 import RgbFusion2

from coolero.models.channel_info import ChannelInfo
from coolero.models.device_info import DeviceInfo
from coolero.models.lighting_mode import LightingMode
from coolero.models.status import TempStatus, ChannelStatus
from coolero.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class RgbFusion2Extractor(LiquidctlDeviceInfoExtractor):
    """This is a lighting only device with no status"""
    supported_driver = RgbFusion2
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []

    @classmethod
    def extract_info(cls, device_instance: RgbFusion2) -> DeviceInfo:
        for channel_name in rgb_fusion2._COLOR_CHANNELS.keys():
            cls._channels[channel_name] = ChannelInfo(
                lighting_modes=cls._get_filtered_color_channel_modes(device_instance)
            )

        cls._lighting_speeds = list(rgb_fusion2._COLOR_CYCLE_SPEEDS.keys())

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, device_instance: RgbFusion2) -> List[LightingMode]:
        channel_modes = []
        for mode_name, (_, _, _, _, _, _, takes_color, speed_values) in rgb_fusion2._COLOR_MODES.items():
            min_colors, max_colors = (1, 1) if takes_color else (0, 0)
            channel_modes.append(LightingMode(
                mode_name, cls._channel_to_frontend_name(mode_name),
                min_colors, max_colors,
                speed_values is not None,
                False
            ))
        return channel_modes

    @classmethod
    def _get_temperatures(cls, status_dict: Dict[str, Any], device_id: int) -> List[TempStatus]:
        return []

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        return []
