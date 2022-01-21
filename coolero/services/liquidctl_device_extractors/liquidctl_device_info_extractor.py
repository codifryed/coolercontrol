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
import re
from collections import defaultdict
from re import Pattern
from typing import Dict, List, Optional, Any, TypeVar, Callable, Union, Tuple

from liquidctl.driver.base import BaseDriver

from models.channel_info import ChannelInfo
from models.device_info import DeviceInfo
from models.lighting_mode import LightingMode
from models.status import Status, TempStatus, ChannelStatus

_LOG = logging.getLogger(__name__)


class LiquidctlDeviceInfoExtractor:
    """This is the base Device Settings class.
    To support a new device simply extend this class and override it's methods and attributes.
    It will be automatically loaded at runtime and the supported_driver will search for available devices.
    """
    T = TypeVar('T')
    supported_driver: BaseDriver = None
    _channels: Dict[str, ChannelInfo] = {}
    _lighting_speeds: List[str] = []
    _pattern_number: Pattern = re.compile(r'\d+')
    _pattern_temp_probe: Pattern = re.compile(r'temperature \d+')
    _pattern_multiple_fan_speed: Pattern = re.compile(r'fan \d+ speed')
    _pattern_multiple_fan_duty: Pattern = re.compile(r'fan \d+ duty')

    @classmethod
    def extract_info(cls, device_instance: BaseDriver) -> DeviceInfo:
        raise NotImplementedError('This should be implemented in one of the child classes')

    @classmethod
    def _get_filtered_color_channel_modes(cls, *args: Any) -> List[LightingMode]:
        raise NotImplementedError('This should be implemented in one of the child classes')

    @classmethod
    def extract_status(cls, status_dict: Dict[str, Union[str, int, float]]) -> Status:
        """default implementation should work for all cases. Subclass implementations are for increase efficiency"""
        return Status(
            firmware_version=cls._get_firmware_ver(status_dict),
            temps=cls._get_temperatures(status_dict),
            channels=cls._get_channel_statuses(status_dict)
        )

    @classmethod
    def _get_firmware_ver(cls, status_dict: Dict[str, Any]) -> Optional[str]:
        value = status_dict.get('firmware version')
        return cls._cast_value_to(value, str)

    @classmethod
    def _get_temperatures(cls, status_dict: Dict[str, Any]) -> List[TempStatus]:
        temps = []
        liquid = cls._get_liquid_temp(status_dict)
        probes = cls._get_temp_probes(status_dict)
        noise_level = cls._get_noise_level(status_dict)
        if liquid is not None:
            temps.append(TempStatus('liquid', liquid))
        for name, temp in probes:
            temps.append(TempStatus(name, temp))
        if noise_level is not None:
            temps.append(TempStatus('noise', noise_level))
        return temps

    @classmethod
    def _get_channel_statuses(cls, status_dict: Dict[str, Any]) -> List[ChannelStatus]:
        """Default implementation that checks for every possibility. Specific Extractors are specialized."""
        channel_statuses: List[ChannelStatus] = []
        fan_rpm = cls._get_fan_rpm(status_dict)
        fan_duty = cls._get_fan_duty(status_dict)
        if fan_rpm is not None or fan_duty is not None:
            channel_statuses.append(ChannelStatus('fan', rpm=fan_rpm, duty=fan_duty))
        pump_rpm = cls._get_pump_rpm(status_dict)
        pump_duty = cls._get_pump_duty(status_dict)
        if pump_rpm is not None or pump_duty is not None:
            channel_statuses.append(ChannelStatus('pump', rpm=pump_rpm, duty=pump_duty))
        multiple_fans_rpm = cls._get_multiple_fans_rpm(status_dict)
        multiple_fans_duty = cls._get_multiple_fans_duty(status_dict)
        multiple_fans: Dict[str, Tuple[Optional[int], Optional[float]]] = defaultdict(lambda: (None, None))
        for name, rpm in multiple_fans_rpm:
            _, set_duty = multiple_fans[name]
            multiple_fans[name] = (rpm, set_duty)
        for name, duty in multiple_fans_duty:
            set_rpm, _ = multiple_fans[name]
            multiple_fans[name] = (set_rpm, duty)
        for name, (rpm, duty) in multiple_fans.items():  # type: ignore[assignment]
            channel_statuses.append(ChannelStatus(name, rpm=rpm, duty=duty))
        return channel_statuses

    @classmethod
    def _get_liquid_temp(cls, status_dict: Dict[str, Any]) -> Optional[float]:
        value = status_dict.get('liquid temperature')
        return cls._cast_value_to(value, float)

    @classmethod
    def _get_temp_probes(cls, status_dict: Dict[str, Any]) -> List[Tuple[str, float]]:
        probes = []
        for name, value in status_dict.items():
            if cls._pattern_temp_probe.match(name):
                temp = cls._cast_value_to(value, float)
                if temp is not None:
                    probe_number = cls._pattern_number.search(name, len(name) - 2).group()
                    probes.append((f'temp{probe_number}', value))
        return probes

    @classmethod
    def _get_noise_level(cls, status_dict: Dict[str, Any]) -> Optional[int]:
        level = status_dict.get('noise level')
        return cls._cast_value_to(level, int)

    @classmethod
    def _get_fan_rpm(cls, status_dict: Dict[str, Any]) -> Optional[int]:
        value = status_dict.get('fan speed')
        return cls._cast_value_to(value, int)

    @classmethod
    def _get_fan_duty(cls, status_dict: Dict[str, Any]) -> Optional[float]:
        value = status_dict.get('fan duty')
        return cls._cast_value_to(value, float)

    @classmethod
    def _get_pump_rpm(cls, status_dict: Dict[str, Any]) -> Optional[int]:
        value = status_dict.get('pump speed')
        return cls._cast_value_to(value, int)

    @classmethod
    def _get_pump_duty(cls, status_dict: Dict[str, Any]) -> Optional[float]:
        value = status_dict.get('pump duty')
        return cls._cast_value_to(value, float)

    @classmethod
    def _get_multiple_fans_rpm(cls, status_dict: Dict[str, Any]) -> List[Tuple[str, int]]:
        fans = []
        for name, value in status_dict.items():
            if cls._pattern_multiple_fan_speed.match(name):
                rpm = cls._cast_value_to(value, int)
                if rpm is not None:
                    fan_number = cls._pattern_number.search(name).group()  # type: ignore[union-attr]
                    fans.append((f'fan{fan_number}', rpm))
        return fans

    @classmethod
    def _get_multiple_fans_duty(cls, status_dict: Dict[str, Any]) -> List[Tuple[str, float]]:
        fans = []
        for name, value in status_dict.items():
            if cls._pattern_multiple_fan_duty.match(name):
                duty = cls._cast_value_to(value, float)
                if duty is not None:
                    fan_number = cls._pattern_number.search(name).group()  # type: ignore[union-attr]
                    fans.append((f'fan{fan_number}', duty))
        return fans

    @classmethod
    def _cast_value_to(cls, value: Any, cast_func: Callable[[Any], T]) -> Optional[T]:
        return cast_func(value) if value is not None else None
