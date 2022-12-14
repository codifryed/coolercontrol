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

from dataclasses import dataclass, field
from enum import Enum

from coolercontrol.models.base_driver import BaseDriver
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.status import Status
from coolercontrol.settings import Settings

STATUS_LENGTH_MAX = 1860  # holds approx. only the last 31 mins of statuses


class DeviceType(str, Enum):
    CPU = "CPU"
    GPU = "GPU"
    LIQUIDCTL = "Liquidctl"
    HWMON = "Hwmon"
    COMPOSITE = "Composite"

    def __str__(self) -> str:
        return str.__str__(self)


@dataclass(unsafe_hash=True)  # special care is taken so that this class is indeed hashable
class Device:
    """This is a model class containing both specific device settings and information"""

    _uid: str
    _name: str
    _type_id: tuple[DeviceType, int]  # a unique ID per device type
    _status_history: list[Status] = field(init=False, default_factory=list, repr=False, compare=False)
    _colors: dict[str, str] = field(default_factory=dict, compare=False)
    _lc_driver_type: BaseDriver | None = None
    _lc_init_firmware_version: str | None = None
    _info: DeviceInfo | None = field(default=None, compare=False)

    @property
    def uid(self) -> str:
        return self._uid

    @property
    def name(self) -> str:
        return self._name

    @property
    def name_short(self) -> str:
        return self._name.partition(' (')[0]

    @property
    def type(self) -> DeviceType:
        return self._type_id[0]

    @property
    def colors(self) -> dict[str, str]:
        return self._colors

    def color(self, channel_name: str) -> str:
        return self._colors.get(channel_name, str(Settings.theme['app_color']['context_color']))

    @property
    def status(self) -> Status:
        assert len(self._status_history) > 0
        return self._status_history[-1]

    @status.setter
    def status(self, status: Status) -> None:
        self._status_history.append(status)
        if len(self._status_history) > STATUS_LENGTH_MAX:
            self._status_history.pop(0)

    @property
    def status_history(self) -> list[Status]:
        return self._status_history

    @status_history.setter
    def status_history(self, statuses: list[Status]) -> None:
        self._status_history.extend(statuses)
        history_size = len(self._status_history)
        if history_size > STATUS_LENGTH_MAX:
            size_diff = history_size - STATUS_LENGTH_MAX
            self._status_history = self._status_history[size_diff:]

    @property
    def lc_driver_type(self) -> BaseDriver | None:
        return self._lc_driver_type

    @property
    def type_id(self) -> int:
        return self._type_id[1]

    @property
    def lc_init_firmware_version(self) -> str | None:
        """On some devices the firmware version only comes on initialization"""
        return self._lc_init_firmware_version

    @property
    def info(self) -> DeviceInfo | None:
        """return the extracted device information, like available channels, color modes, etc"""
        return self._info
