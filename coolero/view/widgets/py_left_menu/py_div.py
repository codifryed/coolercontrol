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

from PySide6.QtWidgets import QWidget, QHBoxLayout, QFrame


class PyDiv(QWidget):
    def __init__(self, color: str) -> None:
        super().__init__()
        self.layout = QHBoxLayout(self)
        self.layout.setContentsMargins(5, 0, 5, 0)
        self.frame_line = QFrame()
        self.frame_line.setStyleSheet(f"background: {color};")
        self.frame_line.setMaximumHeight(1)
        self.frame_line.setMinimumHeight(1)
        self.layout.addWidget(self.frame_line)
        self.setMaximumHeight(1)
