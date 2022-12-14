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

from dataclasses import dataclass
from enum import Enum


class LightingModeType(str, Enum):
    NONE = 'None'
    LC = 'Liquidctl'
    CUSTOM = 'Custom'

    def __str__(self) -> str:
        return str.__str__(self)


@dataclass(frozen=True)
class LightingMode:
    name: str
    frontend_name: str
    min_colors: int
    max_colors: int
    speed_enabled: bool
    backward_enabled: bool
    type: LightingModeType = LightingModeType.LC
