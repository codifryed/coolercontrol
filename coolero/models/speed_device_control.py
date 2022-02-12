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
from typing import List, Dict

from PySide6.QtWidgets import QWidget

from models.speed_profile import SpeedProfile
from models.temp_source import TempSource
from view.uis.controls.ui_speed_control import Ui_SpeedControl


@dataclass(frozen=True)
class SpeedDeviceControl:
    control_widget: QWidget
    control_ui: Ui_SpeedControl
    temp_sources_and_profiles: Dict[TempSource, List[SpeedProfile]] = field(default_factory=dict)
