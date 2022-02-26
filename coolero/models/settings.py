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
from typing import Optional, Tuple, List

from coolero.models.lighting_mode import LightingMode
from coolero.models.temp_source import TempSource


@dataclass
class LightingSettings:
    mode: str
    speed: Optional[str] = None
    backward: bool = False
    colors: List[List[int]] = field(default_factory=list)


@dataclass
class Setting:
    channel_name: str
    speed_fixed: Optional[int] = None
    speed_profile: List[Tuple[int, int]] = field(default_factory=list)
    temp_source: Optional[TempSource] = None
    lighting: Optional[LightingSettings] = None
    lighting_mode: Optional[LightingMode] = None
    last_manual_speeds_set: List[int] = field(default_factory=list)
    under_threshold_counter: int = 0
