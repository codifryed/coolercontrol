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

from __future__ import annotations

from typing import TYPE_CHECKING, Any, no_type_check, Dict

from PySide6.QtCore import Qt

from settings import Settings
from view.widgets import PyGrips

if TYPE_CHECKING:
    from coolero import MainWindow


class SetupMainWindow:

    def __init__(self) -> None:
        super().__init__()
        from view.uis.windows.main_window import UI_MainWindow
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
            self.left_grip = PyGrips(self, "left", self.hide_grips)
            self.right_grip = PyGrips(self, "right", self.hide_grips)
            self.top_grip = PyGrips(self, "top", self.hide_grips)
            self.bottom_grip = PyGrips(self, "bottom", self.hide_grips)
            self.top_left_grip = PyGrips(self, "top_left", self.hide_grips)
            self.top_right_grip = PyGrips(self, "top_right", self.hide_grips)
            self.bottom_left_grip = PyGrips(self, "bottom_left", self.hide_grips)
            self.bottom_right_grip = PyGrips(self, "bottom_right", self.hide_grips)

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
            btn_text="About Coolero",
            btn_tooltip="About Coolero",
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
        self.ui.title_bar.clicked.connect(self.btn_clicked)
        self.ui.title_bar.released.connect(self.btn_released)
        if self.app_settings["custom_title_bar"]:
            self.ui.title_bar.set_title(self.app_settings["app_name"])
        else:
            self.ui.title_bar.set_title("Coolero")

        # left column
        self.ui.left_column.clicked.connect(self.btn_clicked)
        self.ui.left_column.released.connect(self.btn_released)

        # set initial page / set left and right column menus
        from .functions_main_window import MainFunctions
        from view.core.functions import Functions
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
            self.left_grip.setGeometry(5, 10, 10, self.height())
            self.right_grip.setGeometry(self.width() - 15, 10, 10, self.height())
            self.top_grip.setGeometry(5, 5, self.width() - 10, 10)
            self.bottom_grip.setGeometry(5, self.height() - 15, self.width() - 10, 10)
            self.top_right_grip.setGeometry(self.width() - 20, 5, 15, 15)
            self.bottom_left_grip.setGeometry(5, self.height() - 20, 15, 15)
            self.bottom_right_grip.setGeometry(self.width() - 20, self.height() - 20, 15, 15)
