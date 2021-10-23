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
from typing import Dict, List, Optional, Any, TypeVar, Callable, Union

from liquidctl.driver.base import BaseDriver

from models.channel_info import ChannelInfo
from models.device_info import DeviceInfo
from models.lighting_mode import LightingMode
from models.status import Status

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

    @classmethod
    def extract_info(cls, device_instance: BaseDriver) -> DeviceInfo:
        raise NotImplementedError('This should be implemented in one of the child classes')

    @classmethod
    def _get_filtered_color_channel_modes(cls, channel_name: str) -> List[LightingMode]:
        raise NotImplementedError('This should be implemented in one of the child classes')

    @classmethod
    def extract_status(cls, status_dict: Dict[str, Union[str, int, float]]) -> Status:
        """default implementation should suffice for 90% of the cases"""
        return Status(
            liquid_temperature=cls._get_liquid_temp(status_dict),
            firmware_version=cls._get_firmware_ver(status_dict),
            fan_rpm=cls._get_fan_rpm(status_dict),
            fan_duty=cls._get_fan_duty(status_dict),
            pump_rpm=cls._get_pump_rpm(status_dict),
            pump_duty=cls._get_pump_duty(status_dict),
            device_temperature=None,
            load_percent=None,
            device_description=None
        )

    # @classmethod
    # def _device_type_is_supported(cls, device_instance: BaseDriver) -> bool:
    #     """helper method to verify the specific instance type"""
    #     is_supported_type: bool = type(device_instance) == cls.supported_driver
    #     if not is_supported_type:
    #         _LOG.error(f"Something went wrong, incorrect driver, {type(device_instance)}, for this extractor: {cls}")
    #     return is_supported_type

    @classmethod
    def _get_liquid_temp(cls, status_dict: Dict[str, Any]) -> Optional[float]:
        value = status_dict.get('liquid temperature')
        return cls._cast_value_to(value, float)

    @classmethod
    def _get_firmware_ver(cls, status_dict: Dict[str, Any]) -> Optional[str]:
        value = status_dict.get('firmware version')
        return cls._cast_value_to(value, str)

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
    def _cast_value_to(cls, value: Any, cast_func: Callable[[Any], T]) -> Optional[T]:
        return cast_func(value) if value is not None else None
