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
        # layout
        self.theme: Dict = Settings.theme
        self.base_layout = QVBoxLayout(self)
        self.main_layout = QHBoxLayout()
        self.main_layout.setAlignment(Qt.AlignTop)
        self.setStyleSheet('font: 14pt')
        self.label_layout = QVBoxLayout()
        self.main_layout.addLayout(self.label_layout)
        self.switch_layout = QVBoxLayout()
        self.main_layout.addLayout(self.switch_layout)
        self.base_layout.addLayout(self.main_layout)
        self.notes_layout = QVBoxLayout()
        self.notes_layout.setAlignment(Qt.AlignBottom)
        self.base_layout.addLayout(self.notes_layout)
        self.toggle_bg_color = self.theme["app_color"]["dark_two"]
        self.toggle_circle_color = self.theme["app_color"]["icon_color"]
        self.toggle_active_color = self.theme["app_color"]["context_color"]

        self.setting_save_window_size()
        self.setting_enable_light_theme()
        self.setting_hide_on_close()

        self.requires_restart_label = QLabel()
        self.requires_restart_label.setTextFormat(Qt.TextFormat.RichText)
        self.requires_restart_label.setText('<i>*requires restart</i>')
        self.requires_restart_label.setAlignment(Qt.AlignRight)
        self.notes_layout.addWidget(self.requires_restart_label)

    def setting_save_window_size(self) -> None:
        save_window_size_label = QLabel()
        save_window_size_label.setText("Save Window Size on Exit")
        self.label_layout.addWidget(save_window_size_label)
        save_window_size_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.SAVE_WINDOW_SIZE, defaultValue=False, type=bool)
        )
        save_window_size_toggle.setObjectName(UserSettings.SAVE_WINDOW_SIZE)
        save_window_size_toggle.clicked.connect(self.setting_toggled)
        self.switch_layout.addWidget(save_window_size_toggle)

    def setting_enable_light_theme(self) -> None:
        enable_light_theme_label = QLabel()
        enable_light_theme_label.setText('Enable Light Theme*')
        self.label_layout.addWidget(enable_light_theme_label)
        enable_light_theme_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.ENABLE_LIGHT_THEME, defaultValue=False, type=bool)
        )
        enable_light_theme_toggle.setObjectName(UserSettings.ENABLE_LIGHT_THEME)
        enable_light_theme_toggle.clicked.connect(self.setting_toggled)
        self.switch_layout.addWidget(enable_light_theme_toggle)

    def setting_hide_on_close(self) -> None:
        hide_on_close_label = QLabel()
        hide_on_close_label.setText('Minimize To Tray on Close')
        self.label_layout.addWidget(hide_on_close_label)
        hide_on_close_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.HIDE_ON_CLOSE, defaultValue=False, type=bool)
        )
        hide_on_close_toggle.setObjectName(UserSettings.HIDE_ON_CLOSE)
        hide_on_close_toggle.clicked.connect(self.setting_toggled)
        self.switch_layout.addWidget(hide_on_close_toggle)

    @Slot(bool)  # type: ignore[operator]
    def setting_toggled(self, checked: bool) -> None:
        source_btn = self.sender()
        btn_id = source_btn.objectName()
        Settings.user.setValue(btn_id, checked)
