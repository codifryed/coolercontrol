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

from PySide6.QtCore import Qt, QObject
from PySide6.QtGui import QColor
from PySide6.QtWidgets import QFrame, QHBoxLayout, QGraphicsDropShadowEffect

from coolero.settings import Settings
from .styles import Styles


class PyWindow(QFrame):
    def __init__(
            self,
            parent: QObject,
            layout: Qt = Qt.Vertical,
            margin: int = 0,
            spacing: int = 2,
            bg_color: str = "#2c313c",
            text_color: str = "#fff",
            text_font: str = "9pt 'Segoe UI'",
            border_radius: int = 10,
            border_size: int = 2,
            border_color: str = "#343b48",
            enable_shadow: bool = True
    ) -> None:
        super().__init__()
        self.app_settings = Settings.app
        self.parent = parent
        self.layout = layout
        self.margin = margin
        self.bg_color = bg_color
        self.text_color = text_color
        self.text_font = text_font
        self.border_radius = border_radius
        self.border_size = border_size
        self.border_color = border_color
        self.enable_shadow = enable_shadow
        self.setObjectName("pod_bg_app")
        self.set_stylesheet()
        if layout == Qt.Vertical:
            self.layout = QHBoxLayout(self)
        else:
            self.layout = QHBoxLayout(self)
        self.layout.setContentsMargins(margin, margin, margin, margin)
        self.layout.setSpacing(spacing)

        if self.app_settings["custom_title_bar"]:
            if enable_shadow:
                self.shadow = QGraphicsDropShadowEffect()
                self.shadow.setBlurRadius(20)
                self.shadow.setXOffset(0)
                self.shadow.setYOffset(0)
                self.shadow.setColor(QColor(0, 0, 0, 160))
                self.setGraphicsEffect(self.shadow)

    def set_stylesheet(
            self,
            bg_color: str = None,
            border_radius: int = None,
            border_size: int = None,
            border_color: str = None,
            text_color: str = None,
            text_font: str = None
    ) -> None:
        internal_bg_color = bg_color if bg_color is not None else self.bg_color
        if border_radius is not None:
            internal_border_radius = border_radius
        else:
            internal_border_radius = self.border_radius

        if border_size is not None:
            internal_border_size = border_size
        else:
            internal_border_size = self.border_size

        internal_text_color = text_color if text_color is not None else self.text_color
        if border_color is not None:
            internal_border_color = border_color
        else:
            internal_border_color = self.border_color

        internal_text_font = text_font if text_font is not None else self.text_font
        self.setStyleSheet(Styles.bg_style.format(
            _bg_color=internal_bg_color,
            _border_radius=internal_border_radius,
            _border_size=internal_border_size,
            _border_color=internal_border_color,
            _text_color=internal_text_color,
            _text_font=internal_text_font
        ))
