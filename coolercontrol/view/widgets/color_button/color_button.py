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

from typing import Optional, List

from PySide6 import QtGui, QtWidgets
from PySide6.QtCore import Qt, Signal, QEvent, SignalInstance
from PySide6.QtGui import QColor
from PySide6.QtWidgets import QPushButton, QColorDialog

from coolero.dialogs.dialog_style import DIALOG_STYLE
from coolero.settings import Settings


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
                 color: Optional[str] = None,
                 radius: int = 8
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

    def color_rgb(self) -> List[int]:
        return [self._q_color.red(), self._q_color.green(), self._q_color.blue()]

    def on_color_picker(self) -> None:
        dlg = QtWidgets.QColorDialog()
        dlg.setWindowTitle('Select LED Color')
        # AppImage for won't use the native dialog, the Qt one works very well IMHO and allows us to use same UI colors.
        dlg.setOption(QColorDialog.DontUseNativeDialog, True)
        dlg.setStyleSheet(self._dialog_style_sheet)
        # helpful standard custom colors:
        dlg.setCustomColor(0, Qt.red)
        dlg.setCustomColor(1, Qt.green)
        dlg.setCustomColor(2, Qt.blue)
        dlg.setCustomColor(3, Qt.yellow)
        dlg.setCustomColor(4, Qt.cyan)
        dlg.setCustomColor(5, Qt.magenta)
        dlg.setCurrentColor(QtGui.QColor(self._color))
        if dlg.exec_():
            self.set_color(dlg.currentColor().name())

    def mousePressEvent(self, event: QEvent) -> None:
        if event.button() == Qt.RightButton:
            self.set_color(self._default)
        return super().mousePressEvent(event)
