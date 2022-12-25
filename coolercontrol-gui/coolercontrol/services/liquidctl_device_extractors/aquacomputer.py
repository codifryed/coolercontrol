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

from liquidctl.driver.aquacomputer import Aquacomputer

from coolercontrol.models.channel_info import ChannelInfo
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.lighting_mode import LightingMode
from coolercontrol.models.speed_options import SpeedOptions
from coolercontrol.models.status import TempStatus, ChannelStatus
from coolercontrol.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class AquaComputerExtractor(LiquidctlDeviceInfoExtractor):
    supported_driver = Aquacomputer
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_temp = 0
    _max_temp = 100

    @classmethod
    def extract_info(cls, device_instance: Aquacomputer) -> DeviceInfo:
        channel_names: List[str] = []
        controllable_pump_and_fans: Dict | None = device_instance._device_info.get('fan_ctrl')
        if controllable_pump_and_fans is not None:
            channel_names.extend(iter(controllable_pump_and_fans.keys()))
        for channel_name in channel_names:
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=0,
                    max_duty=100,
                    profiles_enabled=False,  # currently none of the aquacomputer devices have this implemented
                    fixed_enabled=True,
                    manual_profiles_enabled=True  # remove if above changes
                )
            )

        # Lighting is not yet implemented for Aquacomputer RGB controllers
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
        liquid_temp = cls._get_liquid_temp(status_dict)
        if liquid_temp is not None:
            temps.append(TempStatus("liquid", liquid_temp, "Liquid", f"LC#{device_id} Liquid"))
        sensors = cls._get_sensors(status_dict)
        temps.extend(
            TempStatus(name, temp, name.capitalize(), f"LC#{device_id} {name.capitalize()}")
            for name, temp in sensors
        )
        return temps

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        channel_statuses: List[ChannelStatus] = []
        pump_rpm = cls._get_pump_rpm(status_dict)
        if pump_rpm is not None:
            channel_statuses.append(ChannelStatus('pump', rpm=pump_rpm))
        single_fan_rpm = cls._get_fan_rpm(status_dict)
        if single_fan_rpm is not None:
            channel_statuses.append(ChannelStatus('fan', rpm=single_fan_rpm))
        multiple_fans_rpm = cls._get_multiple_fans_rpm(status_dict)
        channel_statuses.extend(
            ChannelStatus(name, rpm=rpm)
            for name, rpm in multiple_fans_rpm
        )
        return channel_statuses
