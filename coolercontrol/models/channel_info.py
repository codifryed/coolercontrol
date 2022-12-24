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
from typing import List

from coolero.models.lighting_mode import LightingMode
from coolero.models.speed_options import SpeedOptions
from coolero.models.lcd_mode import LcdMode


@dataclass(frozen=True)
class ChannelInfo:
    speed_options: SpeedOptions | None = None
    lighting_modes: List[LightingMode] = field(default_factory=list, repr=False)
    lcd_modes: List[LcdMode] = field(default_factory=list, repr=False)
