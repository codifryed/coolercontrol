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
from typing import Optional, Dict, Union, Type

from liquidctl.driver.base import BaseDriver

from coolero.models.device_info import DeviceInfo
from coolero.models.status import Status
from coolero.services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)
_RACE_CONDITION_ERROR: str = 'Race condition has removed the driver'


class DeviceExtractor:

    def __init__(self) -> None:
        self._driver_extractors: Dict[Type[BaseDriver], LiquidctlDeviceInfoExtractor] = {
            device_extractor.supported_driver: device_extractor
            for device_extractor in LiquidctlDeviceInfoExtractor.__subclasses__()
        }

    def is_device_supported(self, device: BaseDriver) -> bool:
        if device is not None:
            device_extractor = self._driver_extractors.get(device.__class__)
            is_supported = device_extractor is not None
            if not is_supported:
                _LOG.warning('Device is not supported: %s', device.description)
            return is_supported
        else:
            _LOG.error(_RACE_CONDITION_ERROR)
        return False

    def extract_info_from(self, device: BaseDriver) -> Optional[DeviceInfo]:
        if device is not None:
            device_extractor = self._driver_extractors.get(device.__class__)
            if device_extractor is not None:
                return device_extractor.extract_info(device)
        else:
            _LOG.error(_RACE_CONDITION_ERROR)
        return None

    def extract_status_from(
            self, device: BaseDriver, status_dict: Dict[str, Union[str, int, float]], device_id: int
    ) -> Status:
        if device is not None:
            device_extractor = self._driver_extractors.get(device.__class__)
            if device_extractor is not None:
                return device_extractor.extract_status(status_dict, device_id)
        else:
            _LOG.error(_RACE_CONDITION_ERROR)
        return Status()
