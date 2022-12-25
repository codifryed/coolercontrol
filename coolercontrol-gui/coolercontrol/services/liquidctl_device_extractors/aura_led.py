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

import logging
from typing import List, Dict, Any

from liquidctl.driver import aura_led
from liquidctl.driver.aura_led import AuraLed

from coolercontrol.models.channel_info import ChannelInfo
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.lighting_mode import LightingMode
from coolercontrol.models.status import TempStatus, ChannelStatus
from coolercontrol.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class AuraLedExtractor(LiquidctlDeviceInfoExtractor):
    """This is a lighting only device"""
    supported_driver = AuraLed
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []

    @classmethod
    def extract_info(cls, device_instance: AuraLed) -> DeviceInfo:
        for channel_name in aura_led._COLOR_CHANNELS.keys():
            cls._channels[channel_name] = ChannelInfo(
                lighting_modes=cls._get_filtered_color_channel_modes(device_instance)
            )
        cls._channels['sync'] = ChannelInfo(  # special sync channel
            lighting_modes=cls._get_filtered_color_channel_modes(device_instance)
        )

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, device_instance: AuraLed) -> List[LightingMode]:
        channel_modes = []
        for mode in aura_led._COLOR_MODES.values():
            min_colors, max_colors = (1, 1) if mode.takes_color else (0, 0)
            channel_modes.append(LightingMode(
                name=mode.name,
                frontend_name=cls._channel_to_frontend_name(mode.name),
                min_colors=min_colors,
                max_colors=max_colors,
                speed_enabled=False,
                backward_enabled=False,
            ))
        return channel_modes

    @classmethod
    def _get_temperatures(cls, status_dict: Dict[str, Any], device_id: int) -> List[TempStatus]:
        return []

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        return []
