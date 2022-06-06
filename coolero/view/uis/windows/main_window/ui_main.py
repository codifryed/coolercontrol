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

from typing import no_type_check, Dict

from PySide6.QtWidgets import QWidget, QVBoxLayout, QFrame, QHBoxLayout, QMainWindow

from coolero.settings import Settings
from coolero.view.core.functions import Functions
from coolero.view.uis.canvases.system_overview_canvas import SystemOverviewCanvas
from coolero.view.uis.columns.ui_device_column import Ui_DeviceColumn
from coolero.view.uis.pages.ui_main_pages import Ui_MainPages
from coolero.view.widgets import PyWindow, PyLeftMenu, PyLeftColumn, PyTitleBar


class UI_MainWindow(object):

    @no_type_check
    def __init__(self) -> None:
        self.app_settings: Dict = {}
        self.theme: Dict = {}
        self.central_widget: QWidget = None
        self.central_widget_layout: QVBoxLayout = None
        self.window: PyWindow = None
        self.left_menu_frame: QFrame = None
        self.left_menu_layout: QHBoxLayout = None
        self.left_menu: PyLeftMenu = None
        self.left_column_frame: QFrame = None
        self.left_column_layout: QVBoxLayout = None
        self.left_column: PyLeftColumn = None
        self.right_app_frame: QFrame = None
        self.right_app_layout: QVBoxLayout = None
        self.title_bar_frame: QFrame = None
        self.title_bar_layout: QVBoxLayout = None
        self.title_bar: PyTitleBar = None
        self.content_area_frame: QFrame = None
        self.content_area_layout: QHBoxLayout = None
        self.content_area_left_frame: QFrame = None
        self.load_pages: Ui_MainPages = None
        self.device_column_frame: QFrame = None
        self.device_layout: QVBoxLayout = None
        self.device_bg_frame: QFrame = None
        self.device_column: Ui_DeviceColumn = None
        self.system_overview_canvas: SystemOverviewCanvas = None

    def setup_ui(self, parent: QMainWindow) -> None:
        if not parent.objectName():
            parent.setObjectName("MainWindow")

        self.app_settings = Settings.app
        self.theme = Settings.theme

        parent.resize(self.app_settings["startup_size"][0], self.app_settings["startup_size"][1])
        parent.setMinimumSize(self.app_settings["minimum_size"][0], self.app_settings["minimum_size"][1])

        self.central_widget = QWidget()
        self.central_widget.setStyleSheet(f'''
            font: {self.app_settings["font"]["text_size"]}pt "{self.app_settings["font"]["family"]}";
            color: {self.theme["app_color"]["text_foreground"]};
        ''')
        self.central_widget_layout = QVBoxLayout(self.central_widget)
        if self.app_settings["custom_title_bar"]:
            self.central_widget_layout.setContentsMargins(10, 10, 10, 10)
        else:
            self.central_widget_layout.setContentsMargins(0, 0, 0, 0)

        # Add inside PyWindow "layout" all Widgets
        self.window = PyWindow(
            parent,
            bg_color=self.theme["app_color"]["bg_one"],
            border_color=self.theme["app_color"]["bg_two"],
            text_color=self.theme["app_color"]["text_foreground"]
        )

        # If disable custom title bar
        if not self.app_settings["custom_title_bar"]:
            self.window.set_stylesheet(border_radius=0, border_size=0)

        # add py window to central widget
        self.central_widget_layout.addWidget(self.window)

        # add frame left menu
        left_menu_margin = self.app_settings["left_menu_content_margins"]
        # max and min need to include margins:
        left_menu_maximum = self.app_settings["left_menu_size"]["maximum"] + (left_menu_margin * 2)
        if self.app_settings['left_menu_always_open']:
            left_menu_minimum = left_menu_maximum  # if always open min == max always
        else:
            left_menu_minimum = self.app_settings["left_menu_size"]["minimum"] + (left_menu_margin * 2)
        self.left_menu_frame = QFrame()
        self.left_menu_frame.setMaximumSize(left_menu_minimum, 17280)
        self.left_menu_frame.setMinimumSize(left_menu_minimum, 0)

        # left menu layout
        self.left_menu_layout = QHBoxLayout(self.left_menu_frame)
        self.left_menu_layout.setContentsMargins(
            left_menu_margin,
            left_menu_margin,
            left_menu_margin,
            left_menu_margin
        )

        # add left menu
        self.left_menu = PyLeftMenu(
            parent=self.left_menu_frame,
            app_parent=self.central_widget,  # For tooltip parent
            dark_one=self.theme["app_color"]["dark_one"],
            dark_three=self.theme["app_color"]["dark_three"],
            dark_four=self.theme["app_color"]["dark_four"],
            bg_one=self.theme["app_color"]["bg_one"],
            icon_color=self.theme["app_color"]["icon_color"],
            icon_color_hover=self.theme["app_color"]["icon_hover"],
            icon_color_pressed=self.theme["app_color"]["icon_pressed"],
            icon_color_active=self.theme["app_color"]["icon_active"],
            context_color=self.theme["app_color"]["context_color"],
            text_foreground=self.theme["app_color"]["text_foreground"],
            text_active=self.theme["app_color"]["text_active"],
            minimum_width=left_menu_minimum,
            maximum_width=left_menu_maximum,
            icon_path="logo_color.svg",
            icon_path_close="logo_color.svg",
            toggle_text='Coolero',
            toggle_tooltip='',
        )
        self.left_menu_layout.addWidget(self.left_menu)

        # add left column
        self.left_column_frame = QFrame()
        self.left_column_frame.setMaximumWidth(self.app_settings["left_column_size"]["minimum"])
        self.left_column_frame.setMinimumWidth(self.app_settings["left_column_size"]["minimum"])
        self.left_column_frame.setStyleSheet(f"background: {self.theme['app_color']['bg_two']}")

        # add layout to left column
        self.left_column_layout = QVBoxLayout(self.left_column_frame)
        self.left_column_layout.setContentsMargins(0, 0, 0, 0)

        # add custom left menu widget
        self.left_column = PyLeftColumn(
            parent,
            app_parent=self.central_widget,
            text_title="Settings Left Frame",
            text_title_size=self.app_settings["font"]["title_size"],
            text_title_color=self.theme['app_color']['text_foreground'],
            icon_path=Functions.set_svg_icon("icon_settings.svg"),
            dark_one=self.theme['app_color']['dark_one'],
            bg_color=self.theme['app_color']['bg_three'],
            btn_color=self.theme['app_color']['bg_three'],
            btn_color_hover=self.theme['app_color']['bg_two'],
            btn_color_pressed=self.theme['app_color']['bg_one'],
            icon_color=self.theme['app_color']['icon_color'],
            icon_color_hover=self.theme['app_color']['icon_hover'],
            context_color=self.theme['app_color']['context_color'],
            icon_color_pressed=self.theme['app_color']['icon_pressed'],
            icon_close_path=Functions.set_svg_icon("icon_close.svg")
        )
        self.left_column_layout.addWidget(self.left_column)

        # add right widgets
        self.right_app_frame = QFrame()

        # add right app layout
        self.right_app_layout = QVBoxLayout(self.right_app_frame)
        self.right_app_layout.setContentsMargins(0, 0, 0, 0)
        self.right_app_layout.setSpacing(0)

        # add title bar frame
        self.title_bar_frame = QFrame()

        # add custom title bar to layout
        self.title_bar = PyTitleBar(
            parent,
            logo_width=22,
            app_parent=self.central_widget,
            logo_image="logo_color.svg",
            bg_color=self.theme["app_color"]["bg_two"],
            div_color=self.theme["app_color"]["bg_three"],
            btn_bg_color=self.theme["app_color"]["bg_two"],
            btn_bg_color_hover=self.theme["app_color"]["bg_three"],
            btn_bg_color_pressed=self.theme["app_color"]["bg_one"],
            icon_color=self.theme["app_color"]["icon_color"],
            icon_color_hover=self.theme["app_color"]["icon_hover"],
            icon_color_pressed=self.theme["app_color"]["icon_pressed"],
            icon_color_active=self.theme["app_color"]["icon_active"],
            context_color=self.theme["app_color"]["context_color"],
            dark_one=self.theme["app_color"]["dark_one"],
            text_foreground=self.theme["app_color"]["text_foreground"],
            radius=8,
            font_family=self.app_settings["font"]["family"],
            title_size=self.app_settings["font"]["title_size"],
            title_color=self.theme["app_color"]["text_title"],
            is_custom_title_bar=self.app_settings["custom_title_bar"]
        )

        if self.app_settings["custom_title_bar"]:
            self.title_bar_frame.setMinimumHeight(40)
            self.title_bar_frame.setMaximumHeight(40)
            self.title_bar_layout = QVBoxLayout(self.title_bar_frame)
            self.title_bar_layout.setContentsMargins(0, 0, 0, 0)
            self.title_bar_layout.addWidget(self.title_bar)

        # add content area
        self.content_area_frame = QFrame()

        # create layout
        self.content_area_layout = QHBoxLayout(self.content_area_frame)
        self.content_area_layout.setContentsMargins(0, 0, 0, 0)
        self.content_area_layout.setSpacing(0)

        # left content
        self.content_area_left_frame = QFrame()

        # import main pages to content area
        self.load_pages = Ui_MainPages()
        self.load_pages.setupUi(self.content_area_left_frame)
        # remove all margins since we now use the same bg color
        self.load_pages.main_pages_layout.setContentsMargins(0, 0, 0, 0)
        self.load_pages.system_overview_layout.setContentsMargins(0, 0, 0, 0)
        self.load_pages.system_layout.setContentsMargins(0, 0, 0, 0)

        # add device column
        self.device_column_frame = QFrame()
        self.device_column_frame.setMinimumWidth(0)
        self.device_column_frame.setMaximumWidth(0)
        self.device_column_frame.setStyleSheet(f'''
                    border-radius: 8px;
                    background-color: {self.theme["app_color"]["bg_two"]};
                    margin: 3px;
                ''')
        self.device_layout = QVBoxLayout(self.device_column_frame)
        self.device_layout.setSpacing(0)
        self.device_bg_frame = QFrame()
        self.device_bg_frame.setObjectName("device_bg_frame")
        self.device_bg_frame.setStyleSheet(f'''
                #device_bg_frame {{
                    border-radius: 8px;
                    background-color: {self.theme["app_color"]["bg_two"]};
                }}
                ''')
        self.device_layout.addWidget(self.device_bg_frame)

        self.device_column = Ui_DeviceColumn()
        self.device_column.setupUi(self.device_bg_frame)

        # add to layouts
        self.content_area_layout.addWidget(self.content_area_left_frame)
        self.content_area_layout.addWidget(self.device_column_frame)

        #  REMOVED as there is currently no need to take up space in UI with this. May use later for status updates
        # credits /version / bottom app frame
        # self.credits_frame = QFrame()
        # self.credits_frame.setMinimumHeight(26)
        # self.credits_frame.setMaximumHeight(26)
        # self.credits_layout = QVBoxLayout(self.credits_frame)
        # self.credits_layout.setContentsMargins(0, 0, 0, 0)
        # self.credits = PyCredits(
        #     bg_two=self.theme["app_color"]["bg_two"],
        #     copyright=self.app_settings["copyright"],
        #     version=self.app_settings["version"],
        #     font_family=self.app_settings["font"]["family"],
        #     text_size=self.app_settings["font"]["text_size"],
        #     text_description_color=self.theme["app_color"]["text_description"]
        # )
        # self.credits_layout.addWidget(self.credits)

        # add widgets to right layout
        self.right_app_layout.addWidget(self.title_bar_frame)
        self.right_app_layout.addWidget(self.content_area_frame)
        # self.right_app_layout.addWidget(self.credits_frame)

        # add widgets to "PyWindow"
        self.window.layout.addWidget(self.left_menu_frame)
        self.window.layout.addWidget(self.left_column_frame)
        self.window.layout.addWidget(self.right_app_frame)

        # add central widget and set content margins
        parent.setCentralWidget(self.central_widget)

        # Add system overview chart:
        self.system_overview_canvas = SystemOverviewCanvas()
