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

from PySide6.QtCore import Qt, QRect, QEvent, QObject, QPoint
from PySide6.QtGui import QPainter, QBrush, QColor, QPixmap
from PySide6.QtWidgets import QPushButton, QLabel, QGraphicsDropShadowEffect


class PyLeftButton(QPushButton):
    def __init__(
            self,
            parent: QObject,
            app_parent: Optional[QObject] = None,
            tooltip_text: str = "",
            btn_id: str = None,
            width: int = 30,
            height: int = 30,
            radius: int = 14,
            bg_color: str = "#343b48",
            bg_color_hover: str = "#3c4454",
            bg_color_pressed: str = "#2c313c",
            icon_color: str = "#c3ccdf",
            icon_color_hover: str = "#dce1ec",
            icon_color_pressed: str = "#edf0f5",
            icon_color_active: str = "#f5f6f9",
            icon_path: str = "no_icon.svg",
            dark_one: str = "#1b1e23",
            context_color: str = "#568af2",
            text_foreground: str = "#8a95aa",
            is_active: bool = False
    ) -> None:
        super().__init__()
        self.setFixedSize(width, height)
        self.setCursor(Qt.PointingHandCursor)
        self.setObjectName(btn_id)
        self._bg_color = bg_color
        self._bg_color_hover = bg_color_hover
        self._bg_color_pressed = bg_color_pressed
        self._icon_color = icon_color
        self._icon_color_hover = icon_color_hover
        self._icon_color_pressed = icon_color_pressed
        self._icon_color_active = icon_color_active
        self._context_color = context_color
        self._top_margin = self.height() + 6
        self._is_active = is_active
        self._set_bg_color = bg_color
        self._set_icon_path = icon_path
        self._set_icon_color = icon_color
        self._set_border_radius = radius
        self._parent = parent
        self._app_parent = app_parent
        self._tooltip_text = tooltip_text
        self._tooltip = _ToolTip(
            app_parent,
            tooltip_text,
            dark_one,
            context_color,
            text_foreground
        )
        self._tooltip.hide()

    def set_active(self, is_active: bool) -> None:
        self._is_active = is_active
        self.repaint()

    def is_active(self) -> bool:
        return self._is_active

    def paintEvent(self, event: QEvent) -> None:
        paint = QPainter()
        paint.begin(self)
        paint.setRenderHint(QPainter.RenderHint.Antialiasing)

        if self._is_active:
            brush = QBrush(QColor(self._bg_color_pressed))
        else:
            brush = QBrush(QColor(self._set_bg_color))

        rect = QRect(0, 0, self.width(), self.height())
        paint.setPen(Qt.NoPen)
        paint.setBrush(brush)
        paint.drawRoundedRect(
            rect,
            self._set_border_radius,
            self._set_border_radius
        )

        self.icon_paint(paint, self._set_icon_path, rect)

        paint.end()

    def change_style(self, event: QEvent) -> None:
        if event == QEvent.Enter:
            self._set_bg_color = self._bg_color_hover
            self._set_icon_color = self._icon_color_hover
            self.repaint()
        elif event == QEvent.Leave:
            self._set_bg_color = self._bg_color
            self._set_icon_color = self._icon_color
            self.repaint()
        elif event == QEvent.MouseButtonPress:
            self._set_bg_color = self._bg_color_pressed
            self._set_icon_color = self._icon_color_pressed
            self.repaint()
        elif event == QEvent.MouseButtonRelease:
            self._set_bg_color = self._bg_color_hover
            self._set_icon_color = self._icon_color_hover
            self.repaint()

    def enterEvent(self, event: QEvent) -> None:
        """Event triggered when the mouse is over the btn"""
        self.change_style(QEvent.Enter)
        self.move_tooltip()
        self._tooltip.show()

    def leaveEvent(self, event: QEvent) -> None:
        """Event triggered when the mouse leaves the btn"""
        self.change_style(QEvent.Leave)
        self.move_tooltip()
        self._tooltip.hide()

    def mousePressEvent(self, event: QEvent) -> None:
        if event.button() == Qt.LeftButton:
            self.change_style(QEvent.MouseButtonPress)
            self.setFocus()
            self.clicked.emit()

    def mouseReleaseEvent(self, event: QEvent) -> None:
        if event.button() == Qt.LeftButton:
            self.change_style(QEvent.MouseButtonRelease)
            self.released.emit()

    def icon_paint(self, qp: QPainter, image: str, rect: QRect) -> None:
        """Draw icons with colors"""
        icon = QPixmap(image)
        painter = QPainter(icon)
        painter.setCompositionMode(QPainter.CompositionMode_SourceIn)
        if self._is_active:
            painter.fillRect(icon.rect(), self._context_color)
        else:
            painter.fillRect(icon.rect(), self._set_icon_color)
        qp.drawPixmap(
            (rect.width() - icon.width()) / 2,
            (rect.height() - icon.height()) / 2,
            icon
        )
        painter.end()

    def set_icon(self, icon_path: str) -> None:
        self._set_icon_path = icon_path
        self.repaint()

    def move_tooltip(self) -> None:
        gp = self.mapToGlobal(QPoint(0, 0))

        # set widget to get postion
        pos = self._parent.mapFromGlobal(gp)

        # format position
        pos_x = (pos.x() - self._tooltip.width()) + self.width() + 5
        pos_y = pos.y() + self._top_margin

        # set position to widget
        self._tooltip.move(pos_x, pos_y)


class _ToolTip(QLabel):
    style_tooltip = """ 
    QLabel {{		
        background-color: {_dark_one};	
        color: {_text_foreground};
        padding-left: 10px;
        padding-right: 10px;
        border-radius: 17px;
        border: 0px solid transparent;
        border-right: 3px solid {_context_color};
        font: 800 9pt "Segoe UI";
    }}
    """

    def __init__(
            self,
            parent: Optional[QObject],
            tooltip: str,
            dark_one: str,
            context_color: str,
            text_foreground: str
    ) -> None:
        QLabel.__init__(self)
        style = self.style_tooltip.format(
            _dark_one=dark_one,
            _context_color=context_color,
            _text_foreground=text_foreground
        )
        self.setObjectName(u"label_tooltip")
        self.setStyleSheet(style)
        self.setMinimumHeight(34)
        self.setParent(parent)
        self.setText(tooltip)
        self.adjustSize()
        self.shadow = QGraphicsDropShadowEffect(self)
        self.shadow.setBlurRadius(30)
        self.shadow.setXOffset(0)
        self.shadow.setYOffset(0)
        self.shadow.setColor(QColor(0, 0, 0, 80))
        self.setGraphicsEffect(self.shadow)
