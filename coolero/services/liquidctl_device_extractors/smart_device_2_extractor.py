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
from typing import List, Dict, Any, Tuple, Optional, Set

from liquidctl.driver import smart_device
from liquidctl.driver.smart_device import SmartDevice2

from models.channel_info import ChannelInfo
from models.device_info import DeviceInfo
from models.lighting_mode import LightingMode
from models.speed_options import SpeedOptions
from models.status import TempStatus, ChannelStatus
from services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


# pylint: disable=protected-access
class SmartDevice2Extractor(LiquidctlDeviceInfoExtractor):
    supported_driver = SmartDevice2
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _init_speed_channel_names: Set[str] = set()

    @classmethod
    def extract_info(cls, device_instance: SmartDevice2) -> DeviceInfo:
        for channel_name, (_, min_duty, max_duty) in device_instance._speed_channels.items():
            cls._init_speed_channel_names.add(channel_name)
            cls._channels[channel_name] = ChannelInfo(
                speed_options=SpeedOptions(
                    min_duty=min_duty,
                    max_duty=max_duty,
                    profiles_enabled=False,
                    fixed_enabled=True,
                )
            )
        # special channel 'sync' for all fans
        # currently not supported for SmartDevice2 (only SmartDevice) as this one also has a 'sync' channel for the LEDs

        for channel_name in device_instance._color_channels.keys():
            cls._channels[channel_name] = ChannelInfo(
                lighting_modes=cls._get_filtered_color_channel_modes(device_instance)
            )

        cls._lighting_speeds = list(smart_device._ANIMATION_SPEEDS.keys())

        return DeviceInfo(
            channels=cls._channels,
            lighting_speeds=cls._lighting_speeds,
        )

    @classmethod
    def _get_filtered_color_channel_modes(cls, device_instance: SmartDevice2) -> List[LightingMode]:
        channel_modes = []
        for mode_name, (_, _, moving_byte, min_colors, max_colors) in device_instance._COLOR_MODES.items():
            # todo: direction(backwards) needs to done by hand per mode
            if 'backwards' not in mode_name:  # remove deprecated modes
                channel_modes.append(LightingMode(mode_name, min_colors, max_colors, (moving_byte == 0x10), False))
        return channel_modes

    @classmethod
    def _get_temperatures(cls, status_dict: Dict[str, Any], device_id: int) -> List[TempStatus]:
        # Optional feature for the future:
        # temps = []
        # noise_level = cls._get_noise_level(status_dict)
        # if noise_level is not None:
        #     temps.append(TempStatus('noise', noise_level, 'Noise dB', f'#{device_id} Noise dB'))
        # return temps
        return []

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
        # fan speeds set to 0 will make it disappear from liquidctl status for this driver, (non-0 check)
        #  unfortunately that also happens when no fan is attached.
        if len(multiple_fans.keys()) < len(cls._init_speed_channel_names):
            for speed_channel in cls._init_speed_channel_names:
                if speed_channel not in multiple_fans.keys():
                    channel_statuses.append(ChannelStatus(speed_channel, rpm=0, duty=0.0))
        return channel_statuses
