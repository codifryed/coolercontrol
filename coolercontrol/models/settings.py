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
from typing import Tuple, List

from coolero.models.lcd_mode import LcdMode
from coolero.models.lighting_mode import LightingMode
from coolero.models.temp_source import TempSource


@dataclass
class LightingSettings:
    mode: str
    speed: str | None = None
    backward: bool = False
    colors: List[List[int]] = field(default_factory=list)


@dataclass
class LcdSettings:
    mode: str
    brightness: int | None = None
    orientation: int | None = None
    image_file: str | None = None
    tmp_image_file: str | None = None
    colors: List[List[int]] = field(default_factory=list)


@dataclass
class Setting:
    channel_name: str
    speed_fixed: int | None = None
    speed_profile: List[Tuple[int, int]] = field(default_factory=list)
    temp_source: TempSource | None = None
    lighting: LightingSettings | None = None
    lighting_mode: LightingMode | None = None
    lcd: LcdSettings | None = None
    lcd_mode: LcdMode | None = None
    pwm_mode: int | None = None
    last_manual_speeds_set: List[int] = field(default_factory=list)
    under_threshold_counter: int = 0
