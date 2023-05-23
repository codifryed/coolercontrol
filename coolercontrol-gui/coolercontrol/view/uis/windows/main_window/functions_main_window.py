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

from typing import Any, TYPE_CHECKING

from PySide6.QtCore import QPropertyAnimation, QParallelAnimationGroup, QEasingCurve
from PySide6.QtWidgets import QWidget, QPushButton

if TYPE_CHECKING:
    from coolercontrol.coolercontrol import MainWindow


class MainFunctions:
    left_box = QPropertyAnimation()
    right_box = QPropertyAnimation()
    group = QParallelAnimationGroup()

    # def __init__(self) -> None:
    #     super().__init__()
    #     from coolercontrol.view.uis.windows.main_window import UI_MainWindow
    #     self.ui = UI_MainWindow()
    #     self.ui.setup_ui(self)
    #     self.left_box = QPropertyAnimation()
    #     self.right_box = QPropertyAnimation()
    #     self.group = QParallelAnimationGroup()

    @staticmethod
    def set_page(main_window: 'MainWindow', page: QWidget) -> None:
        """Set main window pages"""
        main_window.ui.load_pages.pages.setCurrentWidget(page)

    @staticmethod
    def set_left_column_menu(
            main_window: 'MainWindow',
            menu: QWidget,
            title: str,
            icon_path: str
    ) -> None:
        """Set left column pages"""
        main_window.ui.left_column.menus.menus.setCurrentWidget(menu)
        main_window.ui.left_column.title_label.setText(title)
        main_window.ui.left_column.icon.set_icon(icon_path)

    @staticmethod
    def left_column_is_visible(main_window: 'MainWindow') -> bool:
        """Return if left column is visible"""
        width: int = main_window.ui.left_column_frame.width()
        return width != 0

    @staticmethod
    def device_column_is_visible(main_window: 'MainWindow') -> bool:
        """Return if device column is visible or not"""
        width: int = main_window.ui.device_column_frame.width()
        return width != 0

    @staticmethod
    def get_title_bar_btn(main_window: 'MainWindow', object_name: str) -> Any:
        """Get Title Button by Object Name"""
        return main_window.ui.title_bar_frame.findChild(QPushButton, object_name)

    @staticmethod
    def get_left_menu_btn(main_window: 'MainWindow', object_name: str) -> Any:
        """Get Left Menu Button by Object Name"""
        return main_window.ui.left_menu.findChild(QPushButton, object_name)

    @staticmethod
    def toggle_left_column(main_window: 'MainWindow') -> None:
        """Toggle animation to Show/Hide left column"""
        left_column_width = main_window.ui.left_column_frame.width()
        device_column_width = main_window.ui.device_column_frame.width()
        MainFunctions.start_box_animation(main_window, left_column_width, device_column_width, "left")

    @staticmethod
    def toggle_device_column(main_window: 'MainWindow') -> None:
        """Toggle animation to Show/Hide device column"""
        left_column_width = main_window.ui.left_column_frame.width()
        device_column_width = main_window.ui.device_column_frame.width()
        MainFunctions.start_box_animation(main_window, left_column_width, device_column_width, "right")

    @staticmethod
    def start_box_animation(main_window: 'MainWindow', left_box_width: int, right_box_width: int, direction: str) -> None:
        right_width: int
        left_width: int
        time_animation = main_window.ui.app_settings["time_animation"]
        minimum_left = main_window.ui.app_settings["left_column_size"]["minimum"]
        maximum_left = main_window.ui.app_settings["left_column_size"]["maximum"]
        minimum_right = 0
        maximum_right = (main_window.size().width() - main_window.ui.left_menu_frame.width()) / 2

        if left_box_width == minimum_left and direction == "left":
            left_width = maximum_left
        else:
            left_width = minimum_left

        if right_box_width == minimum_right and direction == "right":
            right_width = maximum_right
        else:
            right_width = minimum_right

        MainFunctions.left_box = QPropertyAnimation(main_window.ui.left_column_frame, b"minimumWidth")
        MainFunctions.left_box.setDuration(time_animation)
        MainFunctions.left_box.setStartValue(left_box_width)
        MainFunctions.left_box.setEndValue(left_width)
        MainFunctions.left_box.setEasingCurve(QEasingCurve.InOutCubic)

        MainFunctions.right_box = QPropertyAnimation(main_window.ui.device_column_frame, b"minimumWidth")
        MainFunctions.right_box.setDuration(time_animation)
        MainFunctions.right_box.setStartValue(right_box_width)
        MainFunctions.right_box.setEndValue(right_width)
        MainFunctions.right_box.setEasingCurve(QEasingCurve.InOutCubic)

        MainFunctions.group = QParallelAnimationGroup()
        MainFunctions.group.stop()
        MainFunctions.group.addAnimation(MainFunctions.left_box)
        MainFunctions.group.addAnimation(MainFunctions.right_box)
        MainFunctions.group.start()
