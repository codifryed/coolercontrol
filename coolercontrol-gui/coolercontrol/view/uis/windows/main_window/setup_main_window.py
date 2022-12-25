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

from __future__ import annotations

from typing import TYPE_CHECKING, Any, no_type_check, Dict

from PySide6.QtCore import Qt

from coolercontrol.settings import Settings
from coolercontrol.view.widgets import PyGrips

if TYPE_CHECKING:
    from coolercontrol.coolercontrol import MainWindow


class SetupMainWindow:

    def __init__(self) -> None:
        """This init never actually gets called"""
        super().__init__()
        from coolercontrol.view.uis.windows.main_window import UI_MainWindow
        self.ui = UI_MainWindow()
        self.ui.setup_ui(self)
        self.app_settings: Dict = Settings.app
        self.theme: Dict = Settings.theme

    def setup_btns(self) -> Any:
        """Get sender when button is clicked"""
        if self.ui.title_bar.sender() is not None:
            return self.ui.title_bar.sender()
        elif self.ui.left_menu.sender() is not None:
            return self.ui.left_menu.sender()
        elif self.ui.left_column.sender() is not None:
            return self.ui.left_column.sender()

    @no_type_check
    def setup_gui(self) -> None:
        """Setup main window with custom parameters"""
        self.app_settings = Settings.app
        self.theme = Settings.theme
        self.setWindowTitle(self.app_settings["app_name"])
        if self.app_settings["custom_title_bar"]:
            self.setWindowFlag(Qt.FramelessWindowHint)
            self.setAttribute(Qt.WA_TranslucentBackground)
            hide_colorized_grip_borders = True
            self.left_grip = PyGrips(self, "left", hide_colorized_grip_borders)
            self.right_grip = PyGrips(self, "right", hide_colorized_grip_borders)
            self.top_grip = PyGrips(self, "top", hide_colorized_grip_borders)
            self.bottom_grip = PyGrips(self, "bottom", hide_colorized_grip_borders)
            self.top_left_grip = PyGrips(self, "top_left", hide_colorized_grip_borders)
            self.top_right_grip = PyGrips(self, "top_right", hide_colorized_grip_borders)
            self.bottom_left_grip = PyGrips(self, "bottom_left", hide_colorized_grip_borders)
            self.bottom_right_grip = PyGrips(self, "bottom_right", hide_colorized_grip_borders)

        # Standard left menus
        self.ui.left_menu.add_menu_button(
            btn_icon='icon_home.svg',
            btn_id='btn_system',
            btn_text='System Overview',
            btn_tooltip='System Overview',
            show_top=True,
            is_active=True
        )
        self.ui.left_menu.add_menu_button(
            btn_icon="icon_info.svg",
            btn_id="btn_info",
            btn_text="Info",
            btn_tooltip="Info",
            show_top=False,
            is_active=False
        )
        self.ui.left_menu.add_menu_button(
            btn_icon="icon_settings.svg",
            btn_id="btn_settings",
            btn_text="Settings",
            btn_tooltip="Settings",
            show_top=False,
            is_active=False

        )
        self.ui.left_menu.clicked.connect(self.btn_clicked)
        self.ui.left_menu.released.connect(self.btn_released)

        # Title Bar
        if self.ui.app_settings["custom_title_bar"]:
            self.ui.title_bar.clicked.connect(self.btn_clicked)
            self.ui.title_bar.released.connect(self.btn_released)
            self.ui.title_bar.set_title(self.app_settings["app_name"])

        # left column
        self.ui.left_column.clicked.connect(self.btn_clicked)
        self.ui.left_column.released.connect(self.btn_released)

        # set initial page / set left and right column menus
        from .functions_main_window import MainFunctions
        from coolercontrol.view.core.functions import Functions
        MainFunctions.set_page(self, self.ui.load_pages.system_overview)
        MainFunctions.set_left_column_menu(
            self,
            menu=self.ui.left_column.menus.settings_page,
            title="Settings Left Column",
            icon_path=Functions.set_svg_icon("icon_settings.svg")
        )

        # main system overview
        self.ui.load_pages.system_layout.addWidget(self.ui.system_overview_canvas)

    @staticmethod
    def resize_grips(self: MainWindow) -> None:
        if self.app_settings["custom_title_bar"]:
            offset = 5 if self.app_settings["window_shadow"] else 0
            self.left_grip.setGeometry(offset, 10, 10, self.height())
            self.right_grip.setGeometry(self.width() - (10 + offset), 10, 10, self.height())
            self.top_grip.setGeometry(offset, offset, self.width() - 10, 10)
            self.bottom_grip.setGeometry(offset, self.height() - (10 + offset), self.width() - 10, 10)
            self.top_right_grip.setGeometry(self.width() - (15 + offset), offset, 15, 15)
            self.bottom_left_grip.setGeometry(offset, self.height() - (15 + offset), 15, 15)
            self.bottom_right_grip.setGeometry(self.width() - (15 + offset), self.height() - (15 + offset), 15, 15)
