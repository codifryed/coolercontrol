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

from PySide6.QtCore import QSize, Signal, QObject
from PySide6.QtGui import QCursor, Qt, QMouseEvent
from PySide6.QtSvgWidgets import QSvgWidget
from PySide6.QtWidgets import QWidget, QVBoxLayout, QFrame, QHBoxLayout, QLabel

from view.core.functions import Functions
from .py_div import PyDiv
from .py_title_button import PyTitleButton

# GLOBALS
_is_maximized = False
_old_size = QSize()


class PyTitleBar(QWidget):
    clicked = Signal(object)
    released = Signal(object)

    def __init__(
            self,
            parent: QObject,
            app_parent: QObject,
            logo_image: str = "logo_top_100x22.svg",
            logo_width: int = 22,
            dark_one: str = "#1b1e23",
            bg_color: str = "#343b48",
            div_color: str = "#3c4454",
            btn_bg_color: str = "#343b48",
            btn_bg_color_hover: str = "#3c4454",
            btn_bg_color_pressed: str = "#2c313c",
            icon_color: str = "#c3ccdf",
            icon_color_hover: str = "#dce1ec",
            icon_color_pressed: str = "#edf0f5",
            icon_color_active: str = "#f5f6f9",
            context_color: str = "#6c99f4",
            text_foreground: str = "#8a95aa",
            radius: int = 8,
            font_family: str = "Segoe UI",
            title_size: int = 10,
            title_color: str = 'white',
            is_custom_title_bar: bool = True,
    ) -> None:
        super().__init__()
        self._logo_image = logo_image
        self._dark_one = dark_one
        self._bg_color = bg_color
        self._div_color = div_color
        self._parent = parent
        self._app_parent = app_parent
        self._btn_bg_color = btn_bg_color
        self._btn_bg_color_hover = btn_bg_color_hover
        self._btn_bg_color_pressed = btn_bg_color_pressed
        self._context_color = context_color
        self._icon_color = icon_color
        self._icon_color_hover = icon_color_hover
        self._icon_color_pressed = icon_color_pressed
        self._icon_color_active = icon_color_active
        self._font_family = font_family
        self._title_size = title_size
        self._title_color = title_color
        self._text_foreground = text_foreground
        self._is_custom_title_bar = is_custom_title_bar

        self.setup_ui()
        self.bg.setStyleSheet(f"background-color: {bg_color}; border-radius: {radius}px;")
        self.top_logo.setMinimumWidth(logo_width)
        self.top_logo.setMaximumWidth(logo_width)

        # self.top_logo.setPixmap(Functions.set_svg_image(logo_image))

        def move_window(event: QMouseEvent) -> None:
            """Move windows, maximize and restore"""
            if parent.isMaximized():
                # IF MAXIMIZED CHANGE TO NORMAL
                self.maximize_restore()
                self.resize(_old_size)
                curso_x = parent.pos().x()
                curso_y = event.globalPosition().y() - QCursor.pos().y()
                parent.move(curso_x, curso_y)
            if event.buttons() == Qt.LeftButton:
                # MOVE WINDOW
                parent.move(parent.pos() + event.globalPosition().toPoint() - parent.dragPos)
                parent.dragPos = event.globalPosition().toPoint()
                event.accept()

        if is_custom_title_bar:
            # move app widgets
            self.top_logo.mouseMoveEvent = move_window  # type: ignore[assignment]
            self.div_1.mouseMoveEvent = move_window
            self.title_label.mouseMoveEvent = move_window  # type: ignore[assignment]
            self.div_2.mouseMoveEvent = move_window
            self.div_3.mouseMoveEvent = move_window

        if is_custom_title_bar:
            # maximize / restore
            self.top_logo.mouseDoubleClickEvent = self.maximize_restore  # type: ignore[assignment]
            self.div_1.mouseDoubleClickEvent = self.maximize_restore
            self.title_label.mouseDoubleClickEvent = self.maximize_restore  # type: ignore[assignment]
            self.div_2.mouseDoubleClickEvent = self.maximize_restore

        # add widgets to title bar
        self.bg_layout.addWidget(self.top_logo)
        self.bg_layout.addWidget(self.div_1)
        self.bg_layout.addWidget(self.title_label)
        self.bg_layout.addWidget(self.div_2)

        # add buttons buttons
        self.minimize_button.released.connect(lambda: parent.showMinimized())
        self.maximize_restore_button.released.connect(lambda: self.maximize_restore())
        self.close_button.released.connect(lambda: parent.close())

        # Extra BTNs layout
        self.bg_layout.addLayout(self.custom_buttons_layout)

        # ADD Buttons
        if is_custom_title_bar:
            self.bg_layout.addWidget(self.minimize_button)
            self.bg_layout.addWidget(self.maximize_restore_button)
            self.bg_layout.addWidget(self.close_button)

    # def add_menus(self, parameters) -> None:
    #     """Add buttons to title bar and emit signals"""
    #     if parameters is not None and len(parameters) > 0:
    #         for parameter in parameters:
    #             _btn_icon = Functions.set_svg_icon(parameter['btn_icon'])
    #             _btn_id = parameter['btn_id']
    #             _btn_tooltip = parameter['btn_tooltip']
    #             _is_active = parameter['is_active']
    # 
    #             self.menu = PyTitleButton(
    #                 self._parent,
    #                 self._app_parent,
    #                 btn_id=_btn_id,
    #                 tooltip_text=_btn_tooltip,
    #                 dark_one=self._dark_one,
    #                 bg_color=self._bg_color,
    #                 bg_color_hover=self._btn_bg_color_hover,
    #                 bg_color_pressed=self._btn_bg_color_pressed,
    #                 icon_color=self._icon_color,
    #                 icon_color_hover=self._icon_color_active,
    #                 icon_color_pressed=self._icon_color_pressed,
    #                 icon_color_active=self._icon_color_active,
    #                 context_color=self._context_color,
    #                 text_foreground=self._text_foreground,
    #                 icon_path=_btn_icon,
    #                 is_active=_is_active
    #             )
    #             self.menu.clicked.connect(self.btn_clicked)
    #             self.menu.released.connect(self.btn_released)
    # 
    #             # ADD TO LAYOUT
    #             self.custom_buttons_layout.addWidget(self.menu)
    # 
    #         # ADD DIV
    #         if self._is_custom_title_bar:
    #             self.custom_buttons_layout.addWidget(self.div_3)

    def btn_clicked(self) -> None:
        self.clicked.emit(self.menu)

    def btn_released(self) -> None:
        self.released.emit(self.menu)

    def set_title(self, title: str) -> None:
        self.title_label.setText(title)

    def maximize_restore(self, event: Optional[QMouseEvent] = None) -> None:
        """Maximize and restore parent window"""
        global _is_maximized
        global _old_size

        # change ui and resize grip
        def change_ui() -> None:
            if _is_maximized:
                self._parent.ui.central_widget_layout.setContentsMargins(0, 0, 0, 0)
                self._parent.ui.window.set_stylesheet(border_radius=0, border_size=0)
                self.maximize_restore_button.set_icon(
                    Functions.set_svg_icon("icon_restore.svg")
                )
            else:
                self._parent.ui.central_widget_layout.setContentsMargins(10, 10, 10, 10)
                self._parent.ui.window.set_stylesheet(border_radius=10, border_size=2)
                self.maximize_restore_button.set_icon(
                    Functions.set_svg_icon("icon_maximize.svg")
                )

        # check event
        if self._parent.isMaximized():
            _is_maximized = False
            self._parent.showNormal()
            change_ui()
        else:
            _is_maximized = True
            _old_size = QSize(self._parent.width(), self._parent.height())
            self._parent.showMaximized()
            change_ui()

    def setup_ui(self) -> None:
        self.title_bar_layout = QVBoxLayout(self)
        self.title_bar_layout.setContentsMargins(0, 0, 0, 0)
        self.bg = QFrame()
        self.bg_layout = QHBoxLayout(self.bg)
        self.bg_layout.setContentsMargins(10, 0, 5, 0)
        self.bg_layout.setSpacing(0)

        self.div_1 = PyDiv(self._div_color)
        self.div_2 = PyDiv(self._div_color)
        self.div_3 = PyDiv(self._div_color)

        self.top_logo = QLabel()
        self.top_logo_layout = QVBoxLayout(self.top_logo)
        self.top_logo_layout.setContentsMargins(0, 0, 0, 0)
        self.logo_svg = QSvgWidget()
        self.logo_svg.load(Functions.set_svg_image(self._logo_image))
        self.top_logo_layout.addWidget(self.logo_svg, Qt.AlignCenter, Qt.AlignCenter)

        self.title_label = QLabel()
        self.title_label.setAlignment(Qt.AlignVCenter)
        self.title_label.setStyleSheet(f'font: {self._title_size}pt "{self._font_family}"; color: {self._title_color};')

        self.custom_buttons_layout = QHBoxLayout()
        self.custom_buttons_layout.setContentsMargins(0, 0, 0, 0)
        self.custom_buttons_layout.setSpacing(3)

        self.minimize_button = PyTitleButton(
            self._parent,
            self._app_parent,
            tooltip_text="",
            dark_one=self._dark_one,
            bg_color=self._btn_bg_color,
            bg_color_hover=self._btn_bg_color_hover,
            bg_color_pressed=self._btn_bg_color_pressed,
            icon_color=self._icon_color,
            icon_color_hover=self._icon_color_hover,
            icon_color_pressed=self._icon_color_pressed,
            icon_color_active=self._icon_color_active,
            context_color=self._context_color,
            text_foreground=self._text_foreground,
            radius=6,
            icon_path=Functions.set_svg_icon("icon_minimize.svg")
        )

        self.maximize_restore_button = PyTitleButton(
            self._parent,
            self._app_parent,
            tooltip_text="",
            dark_one=self._dark_one,
            bg_color=self._btn_bg_color,
            bg_color_hover=self._btn_bg_color_hover,
            bg_color_pressed=self._btn_bg_color_pressed,
            icon_color=self._icon_color,
            icon_color_hover=self._icon_color_hover,
            icon_color_pressed=self._icon_color_pressed,
            icon_color_active=self._icon_color_active,
            context_color=self._context_color,
            text_foreground=self._text_foreground,
            radius=6,
            icon_path=Functions.set_svg_icon("icon_maximize.svg")
        )

        self.close_button = PyTitleButton(
            self._parent,
            self._app_parent,
            tooltip_text="",
            dark_one=self._dark_one,
            bg_color=self._btn_bg_color,
            bg_color_hover=self._btn_bg_color_hover,
            bg_color_pressed=self._context_color,
            icon_color=self._icon_color,
            icon_color_hover=self._icon_color_hover,
            icon_color_pressed=self._icon_color_active,
            icon_color_active=self._icon_color_active,
            context_color=self._context_color,
            text_foreground=self._text_foreground,
            radius=6,
            icon_path=Functions.set_svg_icon("icon_close.svg")
        )

        self.title_bar_layout.addWidget(self.bg)
