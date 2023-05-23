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

from PySide6.QtCore import Signal, QObject, Qt
from PySide6.QtWidgets import QFrame, QHBoxLayout, QWidget, QVBoxLayout, QLabel

from coolercontrol.view.uis.columns.ui_left_column import Ui_LeftColumn
from coolercontrol.view.widgets.py_left_column.py_icon import PyIcon


class PyLeftColumn(QWidget):
    clicked = Signal(object)
    released = Signal(object)

    def __init__(
            self,
            parent: QObject,
            app_parent: QObject,
            text_title: str,
            text_title_size: int,
            text_title_color: str,
            dark_one: str,
            bg_color: str,
            btn_color: str,
            btn_color_hover: str,
            btn_color_pressed: str,
            icon_path: str,
            icon_color: str,
            icon_color_hover: str,
            icon_color_pressed: str,
            context_color: str,
            icon_close_path: str,
            radius: int = 14
    ) -> None:
        super().__init__()
        self._parent = parent
        self._app_parent = app_parent
        self._text_title = text_title
        self._text_title_size = text_title_size
        self._text_title_color = text_title_color
        self._icon_path = icon_path
        self._dark_one = dark_one
        self._bg_color = bg_color
        self._btn_color = btn_color
        self._btn_color_hover = btn_color_hover
        self._btn_color_pressed = btn_color_pressed
        self._icon_color = icon_color
        self._icon_color_hover = icon_color_hover
        self._icon_color_pressed = icon_color_pressed
        self._context_color = context_color
        self._icon_close_path = icon_close_path
        self._radius = radius

        self.setup_ui()
        self.menus = Ui_LeftColumn()
        self.menus.setupUi(self.content_frame)

    def setup_ui(self) -> None:
        """Widgets"""
        self.base_layout = QVBoxLayout(self)
        self.base_layout.setContentsMargins(0, 0, 0, 0)
        self.base_layout.setSpacing(0)

        self.title_frame = QFrame()
        self.title_frame.setMaximumHeight(47)
        self.title_frame.setMinimumHeight(47)

        self.title_base_layout = QVBoxLayout(self.title_frame)
        self.title_base_layout.setContentsMargins(5, 3, 5, 3)

        self.title_bg_frame = QFrame()
        self.title_bg_frame.setObjectName("title_bg_frame")
        self.title_bg_frame.setStyleSheet(f'''
        #title_bg_frame {{
            background-color: {self._bg_color};
            border-radius: {self._radius}px;
        }}
        ''')

        self.title_bg_layout = QHBoxLayout(self.title_bg_frame)
        self.title_bg_layout.setContentsMargins(5, 5, 5, 5)
        self.title_bg_layout.setSpacing(3)

        self.icon_frame = QFrame()
        self.icon_frame.setFixedSize(30, 30)
        self.icon_frame.setStyleSheet("background: none;")
        self.icon_layout = QVBoxLayout(self.icon_frame)
        self.icon_layout.setContentsMargins(0, 0, 0, 0)
        self.icon_layout.setSpacing(5)
        self.icon = PyIcon(self._icon_path, self._icon_color)
        self.icon_layout.addWidget(self.icon, Qt.AlignCenter, Qt.AlignCenter)

        self.title_label = QLabel(self._text_title)
        self.title_label.setObjectName("title_label")
        self.title_label.setStyleSheet(f'''
        #title_label {{
            font-size: {self._text_title_size}pt;
            color: {self._text_title_color};
            padding-bottom: 2px;
            background: none;
        }}
        ''')

        self.btn_frame = QFrame()
        self.btn_frame.setFixedSize(30, 30)
        self.btn_frame.setStyleSheet("background: none;")

        self.title_bg_layout.addWidget(self.icon_frame)
        self.title_bg_layout.addWidget(self.title_label)
        self.title_bg_layout.addWidget(self.btn_frame)

        self.title_base_layout.addWidget(self.title_bg_frame)

        self.content_frame = QFrame()
        self.content_frame.setStyleSheet("background: none")

        self.base_layout.addWidget(self.title_frame)
        self.base_layout.addWidget(self.content_frame)
