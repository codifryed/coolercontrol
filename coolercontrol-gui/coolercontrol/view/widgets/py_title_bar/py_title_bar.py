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

from PySide6.QtCore import Signal, QObject, QPoint
from PySide6.QtGui import Qt, QMouseEvent
from PySide6.QtSvgWidgets import QSvgWidget
from PySide6.QtWidgets import QWidget, QVBoxLayout, QFrame, QHBoxLayout, QLabel, QMainWindow

from coolercontrol.settings import Settings, UserSettings
from coolercontrol.view.core.functions import Functions
from .py_div import PyDiv
from .py_title_button import PyTitleButton


class PyTitleBar(QWidget):
    clicked = Signal(object)
    released = Signal(object)

    def __init__(
            self,
            parent: QMainWindow,
            app_parent: QObject,
            logo_image: str = 'logo_color.svg',
            logo_width: int = 22,
            logo_size: int = 22,
            radius: int = 8,
    ) -> None:
        super().__init__()
        self._parent: QMainWindow = parent
        self._app_parent: QObject = app_parent
        self._logo_image: str = logo_image
        self._logo_width: int = logo_width
        self._logo_size: int = logo_size
        self._dark_one: str = Settings.theme["app_color"]["dark_one"]
        self._bg_color: str = Settings.theme["app_color"]["bg_two"]
        self._div_color: str = Settings.theme["app_color"]["bg_three"]
        self._btn_bg_color: str = Settings.theme["app_color"]["bg_two"]
        self._btn_bg_color_hover: str = Settings.theme["app_color"]["bg_three"]
        self._btn_bg_color_pressed: str = Settings.theme["app_color"]["bg_one"]
        self._icon_color: str = Settings.theme["app_color"]["icon_color"]
        self._icon_color_hover: str = Settings.theme["app_color"]["icon_hover"]
        self._icon_color_pressed: str = Settings.theme["app_color"]["icon_pressed"]
        self._icon_color_active: str = Settings.theme["app_color"]["icon_active"]
        self._context_color: str = Settings.theme["app_color"]["context_color"]
        self._text_foreground: str = Settings.theme["app_color"]["text_foreground"]
        self._radius: int = radius
        self._font_family: str = Settings.app["font"]["family"]
        self._title_size: str = Settings.app["font"]["title_size"]
        self._title_color: str = Settings.theme["app_color"]["text_title"]
        self._is_custom_title_bar: str = Settings.app["custom_title_bar"]

        self.setup_ui()
        self._is_movable: bool = False
        # self.top_logo.setPixmap(Functions.set_svg_image(logo_image))

        if self._is_custom_title_bar:
            # move app widgets
            self.top_logo.mouseMoveEvent = self.move_window  # type: ignore[assignment]
            self.div_1.mouseMoveEvent = self.move_window  # type: ignore[assignment]
            self.title_label.mouseMoveEvent = self.move_window  # type: ignore[assignment]
            self.div_2.mouseMoveEvent = self.move_window  # type: ignore[assignment]
            self.div_3.mouseMoveEvent = self.move_window  # type: ignore[assignment]
            # maximize / restore
            self.top_logo.mouseDoubleClickEvent = self.maximize_restore
            self.div_1.mouseDoubleClickEvent = self.maximize_restore  # type: ignore[assignment]
            self.title_label.mouseDoubleClickEvent = self.maximize_restore
            self.div_2.mouseDoubleClickEvent = self.maximize_restore  # type: ignore[assignment]

        # add widgets to title bar
        self.bg_layout.addWidget(self.top_logo)
        self.bg_layout.addWidget(self.div_1)
        self.bg_layout.addWidget(self.title_label)
        self.bg_layout.addWidget(self.div_2)

        # add buttons buttons
        self.minimize_button.released.connect(self.minimize_window)
        self.maximize_restore_button.released.connect(self.maximize_restore)
        self.close_button.released.connect(self._parent.close)

        # Extra BTNs layout
        self.bg_layout.addLayout(self.custom_buttons_layout)

        # ADD Buttons
        if self._is_custom_title_bar:
            self.bg_layout.addWidget(self.minimize_button)
            self.bg_layout.addWidget(self.maximize_restore_button)
            self.bg_layout.addWidget(self.close_button)

    # this method adds buttons to the title bar is wanted:
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

    def setup_ui(self) -> None:
        title_bar_layout = QVBoxLayout(self)
        title_bar_layout.setContentsMargins(0, 0, 0, 0)
        bg = QFrame()
        title_bar_layout.addWidget(bg)
        self.bg_layout = QHBoxLayout(bg)
        self.bg_layout.setContentsMargins(10, 0, 5, 0)
        self.bg_layout.setSpacing(0)
        bg.setStyleSheet(f"background-color: {self._bg_color}; border-radius: {self._radius}px;")

        self.div_1 = PyDiv(self._div_color)
        self.div_2 = PyDiv(self._div_color)
        self.div_3 = PyDiv(self._div_color)

        self.top_logo = QLabel()
        self.top_logo_layout = QVBoxLayout(self.top_logo)
        self.top_logo_layout.setContentsMargins(0, 0, 0, 0)
        logo_svg = QSvgWidget()
        logo_svg.load(Functions.set_svg_image(self._logo_image))
        logo_svg.setFixedWidth(self._logo_size)
        logo_svg.setFixedHeight(self._logo_size)
        self.top_logo_layout.addWidget(logo_svg, Qt.AlignCenter, Qt.AlignCenter)
        self.top_logo.setMinimumWidth(self._logo_width)
        self.top_logo.setMaximumWidth(self._logo_width)

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

    def btn_clicked(self) -> None:
        self.clicked.emit(self.menu)  # type: ignore[attr-defined]

    def btn_released(self) -> None:
        self.released.emit(self.menu)  # type: ignore[attr-defined]

    def set_title(self, title: str) -> None:
        self.title_label.setText(f' {title}')

    def move_window(self, event: QMouseEvent) -> None:
        """Move the main window"""
        if self._is_movable and event.buttons() == Qt.LeftButton:
            self._parent.windowHandle().startSystemMove()
            event.accept()

    def maximize_restore(self, event: Optional[QMouseEvent] = None) -> None:
        """Maximize and restore parent window"""
        if self._parent.isMaximized():
            # toggle to normal
            self._parent.showNormal()
            if Settings.app["window_shadow"]:
                self._parent.ui.central_widget_layout.setContentsMargins(10, 10, 10, 10)
            else:
                self._parent.ui.central_widget_layout.setContentsMargins(0, 0, 0, 0)
            self._parent.ui.window.set_stylesheet(border_radius=10, border_size=2)
            self.maximize_restore_button.set_icon(Functions.set_svg_icon("icon_maximize.svg"))
        else:
            # toggle maximized
            if Settings.user.value(UserSettings.SAVE_WINDOW_SIZE, defaultValue=True, type=bool):
                Settings.user.setValue(UserSettings.WINDOW_SIZE, self._parent.size())
                Settings.user.setValue(UserSettings.WINDOW_POSITION, self._parent.pos())
            self._parent.ui.central_widget_layout.setContentsMargins(0, 0, 0, 0)
            self._parent.ui.window.set_stylesheet(border_radius=0, border_size=0)
            self._parent.showMaximized()
            self.maximize_restore_button.set_icon(Functions.set_svg_icon("icon_restore.svg"))

    def minimize_window(self) -> None:
        if Settings.user.value(UserSettings.HIDE_ON_MINIMIZE, defaultValue=False, type=bool):
            self._parent.hide()
        else:
            self._parent.showMinimized()

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.buttons() == Qt.LeftButton:
            self._is_movable = True

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        if event.buttons() == Qt.LeftButton:
            self._is_movable = False
