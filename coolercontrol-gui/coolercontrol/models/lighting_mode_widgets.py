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

from PySide6.QtWidgets import QWidget, QSlider, QCheckBox, QComboBox

from coolercontrol.view.widgets.color_button.color_button import ColorButton
from coolercontrol.view.widgets.image_chooser_button.image_chooser_button import ImageChooserButton


@dataclass
class LightingModeWidgets:
    channel_btn_id: str
    mode: QWidget
    speed: QSlider | None = None
    mode_speeds: list[str] = field(default_factory=list)
    backwards: QCheckBox | None = None
    active_colors: int = 0
    color_buttons: list[ColorButton] = field(default_factory=list)
    file_picker: ImageChooserButton | None = None
    brightness: QSlider | None = None
    orientation: QSlider | None = None
    temp_source: QComboBox | None = None
