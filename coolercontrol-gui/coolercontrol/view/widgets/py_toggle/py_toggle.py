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

from typing import Any

from PySide6.QtCore import QEasingCurve, Qt, QPropertyAnimation, Property, QPoint, QRect
from PySide6.QtGui import QPainter, QFont, QColor, QPaintEvent
from PySide6.QtWidgets import QCheckBox


class PyToggle(QCheckBox):
    def __init__(
            self,
            width: int = 50,
            bg_color: str = "#777",
            circle_color: str = "#DDD",
            active_color: str = "#00BCFF",
            animation_curve: QEasingCurve = QEasingCurve.OutBounce,
            checked: bool = False
    ) -> None:
        QCheckBox.__init__(self)
        self.setFixedSize(width, 28)
        self.setCursor(Qt.PointingHandCursor)
        self._bg_color = bg_color
        self._circle_color = circle_color
        self._active_color = active_color
        self._position = 3
        self.animation = QPropertyAnimation(self, b"position")
        self.animation.setEasingCurve(animation_curve)
        self.animation.setDuration(500)
        self.stateChanged.connect(self.setup_animation)
        self.setChecked(checked)

    @Property(float)
    def position(self):
        return self._position

    @position.setter  # type: ignore[no-redef]
    def position(self, pos: int) -> None:
        self._position = pos
        self.update()

    def setup_animation(self, value: Any) -> None:
        """Start Stop animation"""
        self.animation.stop()
        if value:
            self.animation.setEndValue(self.width() - 26)
        else:
            self.animation.setEndValue(4)
        self.animation.start()

    def hitButton(self, pos: QPoint) -> bool:
        return bool(self.contentsRect().contains(pos))

    def paintEvent(self, event: QPaintEvent) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.setFont(QFont("Segoe UI", 9))

        p.setPen(Qt.NoPen)

        rect = QRect(0, 0, self.width(), self.height())

        if not self.isChecked():
            p.setBrush(QColor(self._bg_color))
        else:
            p.setBrush(QColor(self._active_color))
        p.drawRoundedRect(0, 0, rect.width(), 28, 14, 14)
        p.setBrush(QColor(self._circle_color))
        p.drawEllipse(self._position, 3, 22, 22)
        p.end()
