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

from PySide6.QtCore import QPoint, QEvent, QRect, QObject
from PySide6.QtGui import Qt, QColor, QPainter, QPixmap, QMouseEvent, QFont
from PySide6.QtWidgets import QPushButton, QLabel, QGraphicsDropShadowEffect

from coolercontrol.view.core.functions import Functions
from coolercontrol.settings import UserSettings, Settings


class PyLeftMenuButton(QPushButton):
    def __init__(
            self,
            app_parent: QObject,
            text: str,
            btn_id: str = '',
            tooltip_text: str = "",
            margin: int = 4,
            dark_one: str = "#1b1e23",
            dark_three: str = "#21252d",
            dark_four: str = "#272c36",
            bg_one: str = "#2c313c",
            icon_color: str = "#c3ccdf",
            icon_color_hover: str = "#dce1ec",
            icon_color_pressed: str = "#edf0f5",
            icon_color_active: str = "#f5f6f9",
            context_color: str = "#568af2",
            text_foreground: str = "#8a95aa",
            text_active: str = "#dce1ec",
            icon_path: str = "icon_add_user.svg",
            icon_path_close: str | None = None,
            icon_active_menu: str = "active_menu.svg",
            is_active: bool = False,
            is_active_tab: bool = False,
            is_toggle_active: bool = False,
            is_top_logo_btn: bool = False,
    ) -> None:
        super().__init__()
        self.setText(text)
        self.setCursor(Qt.PointingHandCursor)
        self.setMaximumHeight(50)
        self.setMinimumHeight(50)
        self.setObjectName(btn_id)

        if is_top_logo_btn:
            self._icon_path = Functions.set_svg_image(icon_path)
            self._icon_active_menu = Functions.set_svg_image(icon_path)
        elif icon_path_close is not None and Settings.user.value(  # specifically for open hamburger menu at start
                UserSettings.MENU_OPEN, defaultValue=True, type=bool):
            self._icon_path = Functions.set_svg_icon(icon_path_close)
            self._icon_active_menu = Functions.set_svg_icon(icon_path_close)
            is_toggle_active = True
        else:
            self._icon_path = Functions.set_svg_icon(icon_path)
            self._icon_active_menu = Functions.set_svg_icon(icon_path)

        self._margin = margin
        self._dark_one = dark_one
        self._dark_three = dark_three
        self._dark_four = dark_four
        self._bg_one = bg_one
        self._context_color = context_color
        self._icon_color = icon_color
        self._icon_color_hover = icon_color_hover
        self._icon_color_pressed = icon_color_pressed
        self._icon_color_active = icon_color_active
        self._set_icon_color = self._icon_color  # Set icon color
        self._set_bg_color = self._dark_one  # Set BG color
        self._set_text_foreground = text_foreground
        self._set_text_active = text_active
        self._parent = app_parent
        self._is_active = is_active
        self._is_active_tab = is_active_tab
        self._is_toggle_active = is_toggle_active
        self._is_top_logo_btn: bool = is_top_logo_btn

        self._tooltip_text = tooltip_text
        self.tooltip = _ToolTip(
            app_parent,
            tooltip_text,
            dark_one,
            context_color,
            text_foreground
        )
        self.tooltip.hide()

    def paintEvent(self, event: QEvent) -> None:
        p = QPainter()
        p.begin(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.setPen(Qt.NoPen)
        if self._is_top_logo_btn:
            p.setFont(QFont('Segoe UI', pointSize=14))
        else:
            p.setFont(self.font())

        rect_inside = QRect(4, 5, self.width() - 8, self.height() - 10)
        rect_icon = QRect(0, 0, 50, self.height())
        rect_blue = QRect(4, 5, 20, self.height() - 10)
        rect_inside_active = QRect(7, 5, self.width(), self.height() - 10)
        rect_text = QRect(45, 0, self.width() - 50, self.height())

        if self._is_active:
            # draw bg blue
            p.setBrush(QColor(self._context_color))
            p.drawRoundedRect(rect_blue, 8, 8)

            # bg inside
            p.setBrush(QColor(self._bg_one))
            p.drawRoundedRect(rect_inside_active, 8, 8)

            # draw active
            icon_path = self._icon_active_menu
            self._set_icon_color = self._icon_color_active
            self.icon_active(p, icon_path, self.width())

            # draw text
            p.setPen(QColor(self._set_text_active))
            p.drawText(rect_text, Qt.AlignVCenter, self.text())

            # draw icons
            self.icon_paint(p, self._icon_path, rect_icon, self._set_icon_color)

        elif self._is_active_tab:
            # draw bg blue
            p.setBrush(QColor(self._dark_four))
            p.drawRoundedRect(rect_blue, 8, 8)

            # bg inside
            p.setBrush(QColor(self._bg_one))
            p.drawRoundedRect(rect_inside_active, 8, 8)

            # draw active
            icon_path = self._icon_active_menu
            self._set_icon_color = self._icon_color_active
            self.icon_active(p, icon_path, self.width())

            # draw text
            p.setPen(QColor(self._set_text_active))
            p.drawText(rect_text, Qt.AlignVCenter, self.text())

            # draw icons
            self.icon_paint(p, self._icon_path, rect_icon, self._set_icon_color)

        elif self._is_toggle_active:
            # bg inside
            p.setBrush(QColor(self._dark_three))
            p.drawRoundedRect(rect_inside, 8, 8)

            # draw text
            p.setPen(QColor(self._set_text_foreground))
            p.drawText(rect_text, Qt.AlignVCenter, self.text())

            # draw icons
            if self._is_toggle_active:
                self.icon_paint(p, self._icon_path, rect_icon, self._context_color)
            else:
                self.icon_paint(p, self._icon_path, rect_icon, self._set_icon_color)
        else:
            # bg inside
            p.setBrush(QColor(self._set_bg_color))
            p.drawRoundedRect(rect_inside, 8, 8)

            # draw text
            p.setPen(QColor(self._set_text_foreground))
            p.drawText(rect_text, Qt.AlignVCenter, self.text())

            # draw icons
            self.icon_paint(p, self._icon_path, rect_icon, self._set_icon_color)

        p.end()

    def set_active(self, is_active: bool) -> None:
        """Set active menu"""
        self._is_active = is_active
        if not is_active:
            self._set_icon_color = self._icon_color
            self._set_bg_color = self._dark_one
        self.repaint()

    def set_active_tab(self, is_active: bool) -> None:
        self._is_active_tab = is_active
        if not is_active:
            self._set_icon_color = self._icon_color
            self._set_bg_color = self._dark_one
        self.repaint()

    def is_active(self) -> bool:
        return self._is_active

    def is_active_tab(self) -> bool:
        return self._is_active_tab

    def set_active_toggle(self, is_active: bool) -> None:
        self._is_toggle_active = is_active

    def set_icon(self, icon_path: str) -> None:
        self._icon_path = icon_path
        self.repaint()

    def icon_paint(self, qp: QPainter, image: str, rect: QRect, color: str) -> None:
        """Draw icon with colors"""
        icon = QPixmap(image)
        painter = QPainter(icon)
        painter.setCompositionMode(QPainter.CompositionMode_SourceIn)
        if not self._is_top_logo_btn:  # top logo should use logo colors always
            painter.fillRect(icon.rect(), color)
        qp.drawPixmap(
            (rect.width() - icon.width()) / 2,
            (rect.height() - icon.height()) / 2,
            icon
        )
        painter.end()

    def icon_active(self, qp: QPainter, image: str, width: int) -> None:
        """Draw active icon/ right side"""
        icon = QPixmap(image)
        painter = QPainter(icon)
        painter.setCompositionMode(QPainter.CompositionMode_SourceIn)
        if not self._is_top_logo_btn:  # top logo should use logo colors always
            painter.fillRect(icon.rect(), self._bg_one)
        qp.drawPixmap(width - 5, 0, icon)
        painter.end()

    def change_style(self, event: QEvent) -> None:
        if event in [QEvent.Enter, QEvent.MouseButtonRelease]:
            if not self._is_active:
                self._set_icon_color = self._icon_color_hover
                self._set_bg_color = self._dark_three
            self.repaint()
        elif event == QEvent.Leave:
            if not self._is_active:
                self._set_icon_color = self._icon_color
                self._set_bg_color = self._dark_one
            self.repaint()
        elif event == QEvent.MouseButtonPress:
            if not self._is_active:
                self._set_icon_color = self._context_color
                self._set_bg_color = self._dark_four
            self.repaint()

    def enterEvent(self, event: QMouseEvent) -> None:
        """Event triggered when the mouse is over the button"""
        self.change_style(QEvent.Enter)
        if self.width() == 50 and self._tooltip_text:
            self.move_tooltip()
            self.tooltip.show()

    def leaveEvent(self, event: QMouseEvent) -> None:
        """Event fired when the mouse leaves the button"""
        self.change_style(QEvent.Leave)
        self.tooltip.hide()

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.LeftButton:
            self.change_style(QEvent.MouseButtonPress)
            self.tooltip.hide()
            self.clicked.emit()

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.LeftButton:
            self.change_style(QEvent.MouseButtonRelease)
            self.released.emit()

    def move_tooltip(self) -> None:
        # get main window parent
        gp = self.mapToGlobal(QPoint(0, 0))

        # set widget to get postion
        pos = self._parent.mapFromGlobal(gp)

        # format position
        pos_x = pos.x() + self.width() + 5
        pos_y = pos.y() + (self.width() - self.tooltip.height()) // 2

        # set position to widget
        self.tooltip.move(pos_x, pos_y)


class _ToolTip(QLabel):
    style_tooltip = """ 
    QLabel {{		
        background-color: {_dark_one};	
        color: {_text_foreground};
        padding-left: 10px;
        padding-right: 10px;
        border-radius: 17px;
        border: 0px solid transparent;
        border-left: 3px solid {_context_color};
        font: 400 9pt "Segoe UI";
    }}
    """

    def __init__(
            self,
            parent: QObject,
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
