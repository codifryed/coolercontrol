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
from typing import Optional, Dict, Union

from liquidctl.driver.base import BaseDriver

from models.device_info import DeviceInfo
from models.status import Status
from services.liquidctl_device_extractors import LiquidctlDeviceInfoExtractor

_LOG = logging.getLogger(__name__)


class DeviceExtractor:

    @staticmethod
    def is_device_supported(device: BaseDriver) -> bool:
        is_supported: bool = False
        for device_extractor in LiquidctlDeviceInfoExtractor.__subclasses__():
            if device_extractor.supported_driver is device.__class__:
                is_supported = True
                break
        else:
            if device:
                _LOG.warning("Device is not supported: %s", device.description)
            else:
                _LOG.error("Race condition has removed the driver")
        return is_supported

    @staticmethod
    def extract_info_from(device: BaseDriver) -> Optional[DeviceInfo]:
        for device_extractor in LiquidctlDeviceInfoExtractor.__subclasses__():
            if device_extractor.supported_driver is device.__class__:
                return device_extractor.extract_info(device)
        if device:
            _LOG.error("Driver Instance is not recognized: %s", device.description)
        else:
            _LOG.error("Race condition has removed the driver")
        return None

    @staticmethod
    def extract_status_from(device: BaseDriver, status_dict: Dict[str, Union[str, int, float]]) -> Status:
        for device_extractor in LiquidctlDeviceInfoExtractor.__subclasses__():
            if device_extractor.supported_driver is device.__class__:
                return device_extractor.extract_status(status_dict)
        if device:
            _LOG.error("Driver Instance is not recognized: %s from %s ",
                       device.description,
                       LiquidctlDeviceInfoExtractor.__subclasses__())
        else:
            _LOG.error("Race condition has removed the driver")
        return Status()
