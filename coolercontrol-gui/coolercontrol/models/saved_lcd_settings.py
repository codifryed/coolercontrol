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

from collections import defaultdict
from dataclasses import dataclass, field
from typing import Dict, Tuple, List

from coolercontrol.models.lcd_mode import LcdMode
from coolercontrol.models.saved_speed_settings import DeviceSetting


@dataclass
class LcdModeSetting:
    brightness_slider_value: int | None = None
    orientation_slider_value: int | None = None
    image_file: str | None = None
    tmp_image_file: str | None = None
    active_colors: int | None = None
    button_colors: List[str] = field(default_factory=list)


@dataclass
class LcdModeSettings:
    all: Dict[LcdMode, LcdModeSetting] = field(default_factory=lambda: defaultdict(LcdModeSetting))
    last: Tuple[LcdMode, LcdModeSetting] | None = None


@dataclass
class ChannelLcdSettings:
    channels: Dict[str, LcdModeSettings] = field(default_factory=lambda: defaultdict(LcdModeSettings))


@dataclass
class SavedLcd:
    device_settings: Dict[DeviceSetting, ChannelLcdSettings] = field(
        default_factory=lambda: defaultdict(ChannelLcdSettings))
