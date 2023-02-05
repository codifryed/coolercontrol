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

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout, QFrame, QLabel, QSpacerItem, QSizePolicy


class PyCredits(QWidget):
    def __init__(
            self,
            copyright: str,
            version: str,
            bg_two: str,
            font_family: str,
            text_size: str,
            text_description_color: str,
            radius: int = 8,
            padding: int = 10
    ) -> None:
        super().__init__()
        self._copyright = copyright
        self._version = version
        self._bg_two = bg_two
        self._font_family = font_family
        self._text_size = text_size
        self._text_description_color = text_description_color
        self._radius = radius
        self._padding = padding
        self.setup_ui()

    def setup_ui(self) -> None:
        widget_layout = QHBoxLayout(self)
        widget_layout.setContentsMargins(0, 0, 0, 0)

        style = f"""
        #bg_frame {{
            border-radius: {self._radius}px;
            background-color: {self._bg_two};
        }}
        .QLabel {{
            font: {self._text_size}pt "{self._font_family}";
            color: {self._text_description_color};
            padding-left: {self._padding}px;
            padding-right: {self._padding}px;
        }}
        """

        # bg frame
        bg_frame = QFrame()
        bg_frame.setObjectName("bg_frame")
        bg_frame.setStyleSheet(style)
        widget_layout.addWidget(bg_frame)

        # add bg layout
        bg_layout = QHBoxLayout(bg_frame)
        bg_layout.setContentsMargins(0, 0, 0, 0)

        # add copyright text
        copyright_label = QLabel(self._copyright)
        copyright_label.setAlignment(Qt.AlignVCenter)

        # add version text
        version_label = QLabel('v' + self._version)
        version_label.setAlignment(Qt.AlignVCenter)

        # separator
        separator = QSpacerItem(20, 20, QSizePolicy.Expanding, QSizePolicy.Minimum)

        # add to layout
        bg_layout.addWidget(copyright_label)
        bg_layout.addSpacerItem(separator)
        bg_layout.addWidget(version_label)
