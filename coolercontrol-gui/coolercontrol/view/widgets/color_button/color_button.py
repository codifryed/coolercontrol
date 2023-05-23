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

from PySide6 import QtGui
from PySide6.QtCore import Qt, Signal, QEvent, SignalInstance
from PySide6.QtGui import QColor
from PySide6.QtWidgets import QPushButton

from coolercontrol.dialogs.color_dialog import ColorDialog
from coolercontrol.dialogs.dialog_style import DIALOG_STYLE
from coolercontrol.settings import Settings


class ColorButton(QPushButton):
    _button_style = '''
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
    color_changed: SignalInstance = Signal(object)  # type: ignore

    def __init__(self,
                 color: str | None = None,
                 radius: int = 14
                 ) -> None:
        super().__init__()
        self.setFixedSize(60, 60)
        self.setCursor(Qt.PointingHandCursor)
        self._radius = radius
        self._default: str = '#FFFFFF'
        self._color: str = color or self._default
        self._q_color: QColor = QColor(self._color)
        self.setStyleSheet(self._button_style.format(
            _color=self._color,
            _radius=self._radius,
            _bg_color=self._color,
            _bg_color_hover=self._color,
            _bg_color_pressed=self._color,
        ))
        self._dialog_style_sheet = DIALOG_STYLE.format(
            _text_size=Settings.app["font"]["text_size"],
            _font_family=Settings.app["font"]["family"],
            _text_color=Settings.theme["app_color"]["text_foreground"],
            _bg_color=Settings.theme["app_color"]["bg_one"]
        )
        self.pressed.connect(self.on_color_picker)
        self.set_color(self._color)
        self.active = False

    def set_color(self, color: str) -> None:
        if color != self._color:
            self._color = color
            self._q_color = QColor(color)
            self.color_changed.emit(color)
            self.setStyleSheet(self._button_style.format(
                _color=self._color,
                _radius=self._radius,
                _bg_color=self._color,
                _bg_color_hover=self._color,
                _bg_color_pressed=self._color,
            ))

    def color_hex(self) -> str:
        return self._color

    def color_rgb(self) -> list[int]:
        return [self._q_color.red(), self._q_color.green(), self._q_color.blue()]

    def on_color_picker(self) -> None:
        if not self.active:
            self.active = True
            dlg = ColorDialog()
            dlg.color_picker.setCurrentColor(QtGui.QColor(self._color))
            if dlg.display():
                self.set_color(dlg.color_picker.currentColor().name())
            self.active = False

    def mousePressEvent(self, event: QEvent) -> None:
        if event.button() == Qt.RightButton:
            self.set_color(self._default)
        return super().mousePressEvent(event)
