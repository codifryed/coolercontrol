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

from liquidctl.driver import kraken2
from liquidctl.driver.kraken2 import Kraken2

from coolercontrol.models.channel_info import ChannelInfo
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.lighting_mode import LightingMode
from coolercontrol.models.speed_options import SpeedOptions
from coolercontrol.models.status import TempStatus, ChannelStatus
from coolercontrol.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class Kraken2Extractor(LiquidctlDeviceInfoExtractor):
    supported_driver = Kraken2
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_liquid_temp = 20
    _max_liquid_temp = 60
    _backwards_enabled_modes: List[str] = [
        'spectrum-wave',
        'super-wave',
        'marquee-3',
        'marquee-4',
        'marquee-5',
        'marquee-6',
        'covering-marquee',
        'moving-alternating'
    ]
    _speed_disabled_modes: List[str] = [
        'off',
        'fixed',
        'super-fixed'
    ]

    @classmethod
    def extract_info(cls, device_instance: Kraken2) -> DeviceInfo:
        if device_instance.supports_cooling:
            for channel_name, (_, duty_min, duty_max) in kraken2._SPEED_CHANNELS.items():
                cls._channels[channel_name] = ChannelInfo(
                    speed_options=SpeedOptions(
                        min_duty=duty_min,
                        max_duty=duty_max,
                        profiles_enabled=device_instance.supports_cooling_profiles,
                        fixed_enabled=device_instance.supports_cooling
                    )
                )

        for channel_name in kraken2._COLOR_CHANNELS:
            cls._channels[channel_name] = ChannelInfo(
                lighting_modes=cls._get_filtered_color_channel_modes(channel_name)
            )

        cls._lighting_speeds = list(kraken2._ANIMATION_SPEEDS.keys())

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
            temp_min=cls._min_liquid_temp,
            temp_max=cls._max_liquid_temp,
            profile_max_length=9,
            temp_ext_available=True
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, channel_name: str) -> List[LightingMode]:
        channel_modes: List[LightingMode] = []
        for mode_name, (_, _, _, min_colors, max_colors, only_ring) in kraken2._COLOR_MODES.items():
            if not only_ring or only_ring and channel_name == 'ring':
                if 'backwards' not in mode_name:  # remove any deprecated modes
                    channel_modes.append(LightingMode(
                        mode_name, cls._channel_to_frontend_name(mode_name),
                        min_colors, max_colors,
                        mode_name not in cls._speed_disabled_modes,
                        mode_name in cls._backwards_enabled_modes
                    ))
        return channel_modes

    @classmethod
    def _get_temperatures(cls, status_dict: Dict[str, Any], device_id: int) -> List[TempStatus]:
        temps = []
        liquid = cls._get_liquid_temp(status_dict)
        if liquid is not None:
            temps.append(TempStatus('liquid', liquid, 'Liquid', f'LC#{device_id} Liquid'))
        return temps

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        channel_statuses: List[ChannelStatus] = []
        fan_rpm = cls._get_fan_rpm(status_dict)
        if fan_rpm is not None:
            channel_statuses.append(ChannelStatus('fan', rpm=fan_rpm))
        pump_rpm = cls._get_pump_rpm(status_dict)
        if pump_rpm is not None:
            channel_statuses.append(ChannelStatus('pump', rpm=pump_rpm))
        return channel_statuses
