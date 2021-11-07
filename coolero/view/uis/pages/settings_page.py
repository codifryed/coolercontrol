#  Coolero - monitor and control your cooling and other devices
#  Copyright (c) 2021  Guy Boldon
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

from typing import Dict

from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QLabel

from settings import Settings, UserSettings
from view.widgets import PyToggle


class SettingsPage(QWidget):

    def __init__(self) -> None:
        super().__init__()
        self.theme: Dict = Settings.theme
        self.layout = QHBoxLayout(self)  # type: ignore[assignment]
        self.layout.setAlignment(Qt.AlignTop)
        self.setStyleSheet('font: 14pt')
        self.label_layout = QVBoxLayout()
        self.layout.addLayout(self.label_layout)
        self.switch_layout = QVBoxLayout()
        self.layout.addLayout(self.switch_layout)
        self.save_window_size_label = QLabel()
        self.save_window_size_label.setText("Save Window Size on Exit")
        self.label_layout.addWidget(self.save_window_size_label)
        self.save_window_size_toggle = PyToggle(
            bg_color=self.theme["app_color"]["dark_two"],
            circle_color=self.theme["app_color"]["icon_color"],
            active_color=self.theme["app_color"]["context_color"],
            checked=Settings.user.value(UserSettings.SAVE_WINDOW_SIZE, defaultValue=False, type=bool)
        )
        self.save_window_size_toggle.setObjectName(UserSettings.SAVE_WINDOW_SIZE)
        self.save_window_size_toggle.clicked.connect(self.setting_toggled)
        self.switch_layout.addWidget(self.save_window_size_toggle)

    @Slot(bool)  # type: ignore[operator]
    def setting_toggled(self, checked: bool) -> None:
        source_btn = self.sender()
        btn_id = source_btn.objectName()
        print(f'Button pressed!!!!!!!!!!!!!!!!!!!!!!!! {btn_id} : {checked}')
        Settings.user.setValue(btn_id, checked)
