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
from typing import Optional, Dict, Tuple, List

from models.temp_source import TempSource


@dataclass(frozen=True)
class LightingSettings:
    mode: str
    speed: str
    backward: bool = False
    colors: List[str] = field(default_factory=list)


@dataclass(frozen=True)
class Setting:
    speed_fixed: Optional[int] = None
    speed_profile: List[Tuple[int, int]] = field(default_factory=list)
    profile_temp_source: Optional[TempSource] = None
    lighting: Optional[LightingSettings] = None
    last_manual_speeds_set: List[int] = field(default_factory=list)


@dataclass(frozen=True)
class Settings:
    channel_settings: Dict[str, Setting] = field(default_factory=dict)
