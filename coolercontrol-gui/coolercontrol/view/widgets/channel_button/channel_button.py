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

from typing import Optional

from PySide6.QtCore import QObject
from PySide6.QtGui import Qt
from PySide6.QtWidgets import QToolButton


class ChannelButton(QToolButton):
    _style: str = '''
    QToolButton {{
        border: none;
        padding-left: 10px;
        padding-right: 5px;
        color: {_color};
        border-radius: {_radius};	
        background-color: {_bg_color};
    }}
    QToolButton:hover {{
        background-color: {_bg_color_hover};
    }}
    QToolButton:pressed {{
        background-color: {_bg_color_pressed};
    }}
    QToolButton:checked {{
        color: white;
        background-color: {_active_color};
    }}
    '''

    def __init__(self,
                 color: str,
                 bg_color: str,
                 bg_color_hover: str,
                 text: str = '',
                 radius: int = 14,
                 width: int = 150,
                 height: int = 150,
                 active_color: str = '#568af2',
                 object_name: Optional[str] = None,
                 parent: Optional[QObject] = None,
                 ) -> None:
        super().__init__()
        if parent is not None:
            self.setParent(parent)
        if object_name is not None:
            self.setObjectName(object_name)
        self.setFixedSize(width, height)
        self.setText(text)
        self.setCheckable(True)
        self.setCursor(Qt.PointingHandCursor)
        self.setStyleSheet(self._style.format(
            _color=color,
            _radius=radius,
            _bg_color=bg_color,
            _bg_color_hover=bg_color_hover,
            _bg_color_pressed=bg_color_hover,
            _active_color=active_color,
        ))
