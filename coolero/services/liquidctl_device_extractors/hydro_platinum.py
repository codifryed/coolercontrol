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
from collections import defaultdict
from typing import List, Dict, Any, Optional, Tuple

from liquidctl.driver import hydro_platinum
from liquidctl.driver.hydro_platinum import HydroPlatinum

from coolero.models.channel_info import ChannelInfo
from coolero.models.device_info import DeviceInfo
from coolero.models.lighting_mode import LightingMode
from coolero.models.speed_options import SpeedOptions
from coolero.models.status import TempStatus, ChannelStatus
from coolero.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class HydroPlatinumExtractor(LiquidctlDeviceInfoExtractor):
    supported_driver = HydroPlatinum
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_liquid_temp = 20
    _max_liquid_temp = 60

    @classmethod
    def extract_info(cls, device_instance: HydroPlatinum) -> DeviceInfo:
        for channel_name in device_instance._fan_names:
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=0,
                    max_duty=100,
                    profiles_enabled=True,
                    fixed_enabled=True,
                )
            )

        cls._channels['led'] = ChannelInfo(
            lighting_modes=cls._get_filtered_color_channel_modes(device_instance)
        )

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
            temp_min=cls._min_liquid_temp,
            temp_max=cls._max_liquid_temp,
            temp_ext_available=True,
            profile_max_length=hydro_platinum._PROFILE_LENGTH
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, device_instance: HydroPlatinum) -> List[LightingMode]:
        return [
            LightingMode('off', 'Off', 0, 0, False, False),
            LightingMode('fixed', 'Fixed', 1, 1, False, False),
            LightingMode('super-fixed', 'Super Fixed', 1, device_instance._led_count, False, False),
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
        multiple_fans_rpm = cls._get_multiple_fans_rpm(status_dict)
        multiple_fans_duty = cls._get_multiple_fans_duty(status_dict)
        multiple_fans: Dict[str, Tuple[Optional[int], Optional[float]]] = defaultdict(lambda: (None, None))
        for name, rpm in multiple_fans_rpm:
            _, set_duty = multiple_fans[name]
            multiple_fans[name] = (rpm, set_duty)
        for name, duty in multiple_fans_duty:
            set_rpm, _ = multiple_fans[name]
            multiple_fans[name] = (set_rpm, duty)
        for name, (rpm, duty) in multiple_fans.items():
            channel_statuses.append(ChannelStatus(name, rpm=rpm, duty=duty))
        pump_rpm = cls._get_pump_rpm(status_dict)
        if pump_rpm is not None:
            channel_statuses.append(ChannelStatus('pump', rpm=pump_rpm))
        return channel_statuses
