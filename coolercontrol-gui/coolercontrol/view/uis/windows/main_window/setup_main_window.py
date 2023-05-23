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

from typing import TYPE_CHECKING, Any

from PySide6.QtCore import Qt

if TYPE_CHECKING:
    from coolercontrol.coolercontrol import MainWindow


class SetupMainWindow:

    @staticmethod
    def setup_btns(main_window: MainWindow) -> Any:
        """Get sender when button is clicked"""
        if main_window.ui.title_bar.sender() is not None:
            return main_window.ui.title_bar.sender()
        elif main_window.ui.left_menu.sender() is not None:
            return main_window.ui.left_menu.sender()
        elif main_window.ui.left_column.sender() is not None:
            return main_window.ui.left_column.sender()

    @staticmethod
    def setup_gui(main_window: MainWindow) -> None:
        """Setup main window with custom parameters"""
        if main_window.app_settings["custom_title_bar"]:
            main_window.setWindowFlag(Qt.FramelessWindowHint)
            main_window.setAttribute(Qt.WA_TranslucentBackground)

        # Standard left menus
        main_window.ui.left_menu.add_menu_button(
            btn_icon='icon_home.svg',
            btn_id='btn_system',
            btn_text='System Overview',
            btn_tooltip='System Overview',
            show_top=True,
            is_active=True
        )
        main_window.ui.left_menu.add_menu_button(
            btn_icon="icon_info.svg",
            btn_id="btn_info",
            btn_text="Info",
            btn_tooltip="Info",
            show_top=False,
            is_active=False
        )
        main_window.ui.left_menu.add_menu_button(
            btn_icon="icon_settings.svg",
            btn_id="btn_settings",
            btn_text="Settings",
            btn_tooltip="Settings",
            show_top=False,
            is_active=False

        )
        main_window.ui.left_menu.clicked.connect(main_window.btn_clicked)
        main_window.ui.left_menu.released.connect(main_window.btn_released)

        # left column
        main_window.ui.left_column.clicked.connect(main_window.btn_clicked)
        main_window.ui.left_column.released.connect(main_window.btn_released)

        # set initial page / set left and right column menus
        from .functions_main_window import MainFunctions
        from coolercontrol.view.core.functions import Functions
        MainFunctions.set_page(main_window, main_window.ui.load_pages.system_overview)
        MainFunctions.set_left_column_menu(
            main_window,
            menu=main_window.ui.left_column.menus.settings_page,
            title="Settings Left Column",
            icon_path=Functions.set_svg_icon("icon_settings.svg")
        )

        # main system overview
        main_window.ui.load_pages.system_layout.addWidget(main_window.ui.system_overview_canvas)
