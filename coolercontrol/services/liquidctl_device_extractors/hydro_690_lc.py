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

from liquidctl.driver import asetek
from liquidctl.driver.asetek import Hydro690Lc

from coolero.models.channel_info import ChannelInfo
from coolero.models.device_info import DeviceInfo
from coolero.models.lighting_mode import LightingMode
from coolero.models.speed_options import SpeedOptions
from coolero.models.status import TempStatus, ChannelStatus
from coolero.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class Hydro690Extractor(LiquidctlDeviceInfoExtractor):
    supported_driver = Hydro690Lc
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_liquid_temp = 20
    _max_liquid_temp = 60

    @classmethod
    def extract_info(cls, device_instance: Hydro690Lc) -> DeviceInfo:
        for channel_name, (_, duty_min, duty_max) in asetek._FIXED_SPEED_CHANNELS.items():
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=duty_min,
                    max_duty=duty_max,
                    profiles_enabled=False,
                    fixed_enabled=True,
                    manual_profiles_enabled=True
                )
            )
        for channel_name, (_, duty_min, duty_max) in asetek._VARIABLE_SPEED_CHANNELS.items():
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=duty_min,
                    max_duty=duty_max,
                    profiles_enabled=True,
                    fixed_enabled=True,
                )
            )

        cls._channels['LED'] = ChannelInfo(
            lighting_modes=cls._get_filtered_color_channel_modes('LED')
        )

        cls._lighting_speeds = [str(i) for i in range(5, 0, -1)]

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
            temp_min=cls._min_liquid_temp,
            temp_max=cls._max_liquid_temp,
            temp_ext_available=True,
            profile_max_length=6,
            profile_min_length=2
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, channel_name: str) -> List[LightingMode]:
        # are done by hand for this device
        # alert temp is also supported for this device
        return [
            LightingMode('blackout', 'Blackout', 0, 0, False, False),
            LightingMode('fixed', 'Fixed', 1, 1, False, False),
            LightingMode('fading', 'Fading', 1, 2, True, False),
            LightingMode('blinking', 'Blinking', 1, 1, True, False),
        ]

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
