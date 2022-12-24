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

from liquidctl.driver.corsair_hid_psu import CorsairHidPsu

from coolercontrol.models.channel_info import ChannelInfo
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.lighting_mode import LightingMode
from coolercontrol.models.speed_options import SpeedOptions
from coolercontrol.models.status import TempStatus, ChannelStatus
from coolercontrol.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class CorsairHidPSUExtractor(LiquidctlDeviceInfoExtractor):
    supported_driver = CorsairHidPsu
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _min_temp = 20
    _max_temp = 100

    @classmethod
    def extract_info(cls, device_instance: CorsairHidPsu) -> DeviceInfo:
        cls._channels['fan'] = ChannelInfo(
            speed_options=SpeedOptions(
                min_duty=30,
                max_duty=100,
                profiles_enabled=False,
                fixed_enabled=True,
                manual_profiles_enabled=True
            )
        )

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
        vrm_temp = cls._get_vrm_temp(status_dict)
        if vrm_temp is not None:
            temps.append(TempStatus("vrm", vrm_temp, "VRM", f"LC#{device_id} VRM"))
        case_temp = cls._get_case_temp(status_dict)
        if case_temp is not None:
            temps.append(TempStatus("case", case_temp, "Case", f"LC#{device_id} Case"))
        return temps

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        channel_statuses: List[ChannelStatus] = []
        fan_rpm = cls._get_fan_rpm(status_dict)
        if fan_rpm is not None:
            channel_statuses.append(ChannelStatus('fan', rpm=fan_rpm))
        return channel_statuses
