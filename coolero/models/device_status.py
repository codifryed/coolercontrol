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

from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import Optional, List

from liquidctl.driver.base import BaseDriver

from models.device_info import DeviceInfo
from models.status import Status


@dataclass
class DeviceStatus:
    """This is a model class containing both specific device settings and information"""

    _device_name: str
    _status_current: Status = field(compare=False)
    _status_history: List[Status] = field(init=False, default_factory=list, compare=False)
    _lc_device_id: Optional[int] = None
    _lc_driver_type: Optional[BaseDriver] = None
    _lc_init_firmware_version: Optional[str] = None
    _device_info: Optional[DeviceInfo] = None

    @property
    def device_name(self) -> str:
        return self._device_name

    @property
    def device_name_short(self) -> str:
        return self._device_name.partition(' (')[0]

    @property
    def status(self) -> Status:
        return self._status_current

    @status.setter
    def status(self, status: Status) -> None:
        self._status_current = status
        self._append_status_to_history(status)

    @property
    def status_history(self) -> List[Status]:
        return self._status_history

    @property
    def lc_driver_type(self) -> Optional[BaseDriver]:
        return self._lc_driver_type

    @property
    def lc_device_id(self) -> Optional[int]:
        return self._lc_device_id

    @property
    def lc_init_firmware_version(self) -> Optional[str]:
        """On some devices the firmware version only comes on initialization"""
        return self._lc_init_firmware_version

    @property
    def device_info(self) -> Optional[DeviceInfo]:
        """return the extracted device information, like available channels, color modes, etc"""
        return self._device_info

    def _append_status_to_history(self, status: Status) -> None:
        self._status_history.append(status)
        time_delta: timedelta = datetime.now() - self._status_history[0].timestamp
        if time_delta.days > 0:  # remove status history if older than a day to keep list from exploding
            self._status_history.pop(0)
