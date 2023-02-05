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

from PySide6.QtCore import QPropertyAnimation, QParallelAnimationGroup, QEasingCurve
from PySide6.QtWidgets import QWidget, QPushButton


class MainFunctions:
    def __init__(self) -> None:
        super().__init__()
        from coolercontrol.view.uis.windows.main_window import UI_MainWindow
        self.ui = UI_MainWindow()
        self.ui.setup_ui(self)
        self.left_box = QPropertyAnimation()
        self.right_box = QPropertyAnimation()
        self.group = QParallelAnimationGroup()

    def set_page(self, page: QWidget) -> None:
        """Set main window pages"""
        self.ui.load_pages.pages.setCurrentWidget(page)

    def set_left_column_menu(
            self,
            menu: QWidget,
            title: str,
            icon_path: str
    ) -> None:
        """Set left column pages"""
        self.ui.left_column.menus.menus.setCurrentWidget(menu)
        self.ui.left_column.title_label.setText(title)
        self.ui.left_column.icon.set_icon(icon_path)

    def left_column_is_visible(self) -> bool:
        """Return if left column is visible"""
        width: int = self.ui.left_column_frame.width()
        return width != 0

    def device_column_is_visible(self) -> bool:
        """Return if device column is visible or not"""
        width: int = self.ui.device_column_frame.width()
        return width != 0

    def get_title_bar_btn(self, object_name: str) -> Any:
        """Get Title Button by Object Name"""
        return self.ui.title_bar_frame.findChild(QPushButton, object_name)

    def get_left_menu_btn(self, object_name: str) -> Any:
        """Get Left Menu Button by Object Name"""
        return self.ui.left_menu.findChild(QPushButton, object_name)

    def toggle_left_column(self) -> None:
        """Toggle animation to Show/Hide left column"""
        left_column_width = self.ui.left_column_frame.width()
        device_column_width = self.ui.device_column_frame.width()
        MainFunctions.start_box_animation(self, left_column_width, device_column_width, "left")

    def toggle_device_column(self) -> None:
        """Toggle animation to Show/Hide device column"""
        left_column_width = self.ui.left_column_frame.width()
        device_column_width = self.ui.device_column_frame.width()
        MainFunctions.start_box_animation(self, left_column_width, device_column_width, "right")

    def start_box_animation(self, left_box_width: int, right_box_width: int, direction: str) -> None:
        right_width: int
        left_width: int
        time_animation = self.ui.app_settings["time_animation"]
        minimum_left = self.ui.app_settings["left_column_size"]["minimum"]
        maximum_left = self.ui.app_settings["left_column_size"]["maximum"]
        minimum_right = 0
        maximum_right = (self.size().width() - self.ui.left_menu_frame.width()) / 2

        if left_box_width == minimum_left and direction == "left":
            left_width = maximum_left
        else:
            left_width = minimum_left

        if right_box_width == minimum_right and direction == "right":
            right_width = maximum_right
        else:
            right_width = minimum_right

        self.left_box = QPropertyAnimation(self.ui.left_column_frame, b"minimumWidth")
        self.left_box.setDuration(time_animation)
        self.left_box.setStartValue(left_box_width)
        self.left_box.setEndValue(left_width)
        self.left_box.setEasingCurve(QEasingCurve.InOutCubic)

        self.right_box = QPropertyAnimation(self.ui.device_column_frame, b"minimumWidth")
        self.right_box.setDuration(time_animation)
        self.right_box.setStartValue(right_box_width)
        self.right_box.setEndValue(right_width)
        self.right_box.setEasingCurve(QEasingCurve.InOutCubic)

        self.group = QParallelAnimationGroup()
        self.group.stop()
        self.group.addAnimation(self.left_box)
        self.group.addAnimation(self.right_box)
        self.group.start()
