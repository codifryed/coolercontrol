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

from liquidctl.driver import asetek_pro
from liquidctl.driver.asetek_pro import HydroPro

from coolero.models.channel_info import ChannelInfo
from coolero.models.device_info import DeviceInfo
from coolero.models.lighting_mode import LightingMode
from coolero.models.speed_options import SpeedOptions
from coolero.models.status import TempStatus, ChannelStatus
from coolero.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class HydroProExtractor(LiquidctlDeviceInfoExtractor):
    supported_driver = HydroPro
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_liquid_temp = 20
    _max_liquid_temp = 60

    @classmethod
    def extract_info(cls, device_instance: HydroPro) -> DeviceInfo:
        cls._channels['pump'] = ChannelInfo(
            speed_options=SpeedOptions(
                min_duty=20,
                max_duty=100,
                profiles_enabled=False,
                fixed_enabled=True,
                manual_profiles_enabled=False
            )
        )
        channels_names = [f'fan{fan_number + 1}' for fan_number in range(device_instance._fan_count)]
        for channel_name in channels_names:
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=0,
                    max_duty=100,
                    profiles_enabled=True,
                    fixed_enabled=True,
                )
            )

        cls._channels['logo'] = ChannelInfo(
            lighting_modes=cls._get_filtered_color_channel_modes('logo')
        )

        cls._lighting_speeds = asetek_pro._COLOR_SPEEDS

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
            temp_min=cls._min_liquid_temp,
            temp_max=cls._max_liquid_temp,
            temp_ext_available=True,
            profile_max_length=7,
            profile_min_length=2
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, channel_name: str) -> List[LightingMode]:
        # are done by hand for this device
        return [
            LightingMode('fixed', 'Fixed', 1, 1, False, False),
            LightingMode('blinking', 'Blinking', 1, 4, True, False),
            LightingMode('pulse', 'Pulse', 1, 4, True, False),
            LightingMode('shift', 'Shift', 2, 4, True, False),
            LightingMode('alert', 'Alert', 3, 3, False, False)
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
        channel_statuses.extend(
            ChannelStatus(name, rpm=rpm)
            for name, rpm in multiple_fans_rpm
        )
        pump_rpm = cls._get_pump_rpm(status_dict)
        pump_mode = cls._get_pump_mode(status_dict)
        pump_duty = None
        if pump_mode is not None:
            if pump_mode == 'balanced':
                pump_duty = 50
            elif pump_mode == 'performance':
                pump_duty = 75
            elif pump_mode == 'quiet':
                pump_duty = 25
        if pump_rpm is not None:
            channel_statuses.append(ChannelStatus('pump', rpm=pump_rpm, duty=pump_duty))
        return channel_statuses
