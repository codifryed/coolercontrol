#  Coolero - monitor and control your cooling and other devices
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

from collections import defaultdict
from dataclasses import dataclass, field
from typing import Dict, Tuple, Optional, List

from models.lighting_mode import LightingMode


@dataclass
class ModeSetting:
    speed_slider_value: Optional[int] = None
    backwards: bool = False
    active_colors: Optional[int] = None
    button_colors: List[str] = field(default_factory=list)


@dataclass
class ModeSettings:
    all: Dict[LightingMode, ModeSetting] = field(default_factory=lambda: defaultdict(ModeSetting))
    last: Optional[Tuple[LightingMode, ModeSetting]] = None


@dataclass
class ChannelLightingSettings:
    channels: Dict[str, ModeSettings] = field(default_factory=lambda: defaultdict(ModeSettings))


@dataclass
class SavedLighting:
    device_settings: Dict[int, ChannelLightingSettings] = field(
        default_factory=lambda: defaultdict(ChannelLightingSettings))
