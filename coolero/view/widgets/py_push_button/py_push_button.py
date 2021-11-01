#  Coolero - monitor and control your cooling and other devices
#  Copyright (c) 2021  Guy Boldon
#  All credit for basis of the user interface (GUI) goes to: Wanderson M.Pimenta
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

from typing import Optional

from PySide6.QtCore import Qt, QObject
from PySide6.QtWidgets import QPushButton


class PyPushButton(QPushButton):
    _style = '''
    QPushButton {{
        border: none;
        padding-left: 10px;
        padding-right: 5px;
        color: {_color};
        border-radius: {_radius};	
        background-color: {_bg_color};
    }}
    QPushButton:hover {{
        background-color: {_bg_color_hover};
    }}
    QPushButton:pressed {{	
        background-color: {_bg_color_pressed};
    }}
    '''

    def __init__(self,
                 color: str,
                 bg_color: str,
                 bg_color_hover: str,
                 bg_color_pressed: str,
                 text: str = '',
                 radius: int = 8,
                 parent: Optional[QObject] = None,
                 ) -> None:
        super().__init__()
        if parent is not None:
            self.setParent(parent)
        self.setText(text)
        self.setCursor(Qt.PointingHandCursor)
        self.setStyleSheet(self._style.format(
            _color=color,
            _radius=radius,
            _bg_color=bg_color,
            _bg_color_hover=bg_color_hover,
            _bg_color_pressed=bg_color_pressed
        ))
