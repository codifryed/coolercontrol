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

from PySide6.QtCore import Qt
from PySide6.QtGui import QPixmap, QPainter
from PySide6.QtWidgets import QWidget, QVBoxLayout, QLabel


class PyIcon(QWidget):
    def __init__(
            self,
            icon_path: str,
            icon_color: str
    ) -> None:
        super().__init__()
        self._icon_path = icon_path
        self._icon_color = icon_color
        self.setup_ui()

    def setup_ui(self) -> None:
        self.layout = QVBoxLayout(self)  # type: ignore[assignment]
        self.layout.setContentsMargins(0, 0, 0, 0)
        self.icon = QLabel()
        self.icon.setAlignment(Qt.AlignCenter)
        self.set_icon(self._icon_path, self._icon_color)
        self.layout.addWidget(self.icon)

    def set_icon(self, icon_path: str, icon_color: Optional[str] = None) -> None:
        color: str = icon_color if icon_color is not None else self._icon_color
        icon = QPixmap(icon_path)
        painter = QPainter(icon)
        painter.setCompositionMode(QPainter.CompositionMode_SourceIn)
        painter.fillRect(icon.rect(), color)
        painter.end()
        self.icon.setPixmap(icon)
