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

from coolercontrol.models.temp_source import TempSource


@dataclass
class ClipboardBuffer:
    temp_source: TempSource | None = None
    _profile_temps: list[int] = field(init=False, default_factory=list)
    _profile_duties: list[int] = field(init=False, default_factory=list)

    @property
    def is_full(self) -> bool:
        return self.temp_source is not None and self._profile_temps and self._profile_duties

    @property
    def profile_temps(self) -> list[int]:
        return list(self._profile_temps)

    @profile_temps.setter
    def profile_temps(self, temps: list[int]) -> None:
        self._profile_temps = list(temps)

    @property
    def profile_duties(self) -> list[int]:
        return list(self._profile_duties)

    @profile_duties.setter
    def profile_duties(self, duties: list[int]) -> None:
        self._profile_duties = list(duties)
