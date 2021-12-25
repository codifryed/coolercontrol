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

from PySide6.QtCore import Qt, Slot, QMargins
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QLabel, QFrame, QSpacerItem

from settings import Settings, UserSettings
from view.widgets import PyToggle, PySlider


class SettingsPage(QWidget):

    def __init__(self) -> None:
        super().__init__()
        # layout
        self.theme: Dict = Settings.theme
        self.setStyleSheet('font: 14pt')
        self.base_layout = QVBoxLayout(self)
        self.toggle_bg_color = self.theme["app_color"]["dark_two"]
        self.toggle_circle_color = self.theme["app_color"]["icon_color"]
        self.toggle_active_color = self.theme["app_color"]["context_color"]

        self.setting_save_window_size()
        self.base_layout.addItem(self.spacer())
        self.setting_hide_on_close()
        self.base_layout.addWidget(self.line())
        self.setting_enable_light_theme()
        self.base_layout.addItem(self.spacer())
        self.setting_ui_scaling()

        self.notes_layout = QVBoxLayout()
        self.notes_layout.setAlignment(Qt.AlignBottom)
        self.requires_restart_label = QLabel()
        self.requires_restart_label.setTextFormat(Qt.TextFormat.RichText)
        self.requires_restart_label.setText('<i><b>*</b>requires restart</i>')
        self.requires_restart_label.setAlignment(Qt.AlignRight)
        self.notes_layout.addWidget(self.requires_restart_label)
        self.base_layout.addLayout(self.notes_layout)

    @staticmethod
    def line() -> QFrame:
        return QFrame(  # type: ignore[call-arg]
            frameShape=QFrame.HLine, frameShadow=QFrame.Plain, minimumHeight=30, contentsMargins=QMargins(40, 0, 40, 0)
        )

    @staticmethod
    def spacer() -> QSpacerItem:
        return QSpacerItem(1, 10)

    def setting_save_window_size(self) -> None:
        save_window_size_layout = QHBoxLayout()
        save_window_size_label = QLabel(text='Save Window Status on Exit')
        save_window_size_layout.addWidget(save_window_size_label)
        save_window_size_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.SAVE_WINDOW_SIZE, defaultValue=False, type=bool)
        )
        save_window_size_toggle.setObjectName(UserSettings.SAVE_WINDOW_SIZE)
        save_window_size_toggle.clicked.connect(self.setting_toggled)
        save_window_size_layout.addWidget(save_window_size_toggle)
        self.base_layout.addLayout(save_window_size_layout)

    def setting_hide_on_close(self) -> None:
        hide_on_close_layout = QHBoxLayout()
        hide_on_close_label = QLabel(text='Close to Tray')
        hide_on_close_layout.addWidget(hide_on_close_label)
        hide_on_close_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.HIDE_ON_CLOSE, defaultValue=False, type=bool)
        )
        hide_on_close_toggle.setObjectName(UserSettings.HIDE_ON_CLOSE)
        hide_on_close_toggle.clicked.connect(self.setting_toggled)
        hide_on_close_layout.addWidget(hide_on_close_toggle)
        self.base_layout.addLayout(hide_on_close_layout)

    def setting_enable_light_theme(self) -> None:
        enable_light_theme_layout = QHBoxLayout()
        enable_light_theme_label = QLabel(text='<b>*</b>Enable Light Theme')
        enable_light_theme_layout.addWidget(enable_light_theme_label)
        enable_light_theme_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.ENABLE_LIGHT_THEME, defaultValue=False, type=bool)
        )
        enable_light_theme_toggle.setObjectName(UserSettings.ENABLE_LIGHT_THEME)
        enable_light_theme_toggle.clicked.connect(self.setting_toggled)
        enable_light_theme_layout.addWidget(enable_light_theme_toggle)
        self.base_layout.addLayout(enable_light_theme_layout)

    def setting_ui_scaling(self) -> None:
        ui_scaling_layout = QVBoxLayout()
        ui_scaling_layout.setAlignment(Qt.AlignTop)
        ui_scaling_label = QLabel(text='<b>*</b>UI Scaling Factor')
        ui_scaling_layout.addWidget(ui_scaling_label)
        ui_scaling_slider = PySlider(
            bg_color=self.toggle_bg_color,
            bg_color_hover=self.toggle_bg_color,
            handle_color=self.toggle_circle_color,
            handle_color_hover=self.toggle_active_color,
            handle_color_pressed=self.toggle_active_color,
            orientation=Qt.Orientation.Horizontal,
            tickInterval=1, singleStep=1, minimum=0, maximum=4
        )
        ui_scaling_slider.setObjectName(UserSettings.UI_SCALE_FACTOR)
        ui_scaling_slider.valueChanged.connect(lambda: self.setting_slider_changed(ui_scaling_slider))
        ui_scaling_slider.setValue(
            self.convert_scale_factor_to_slider_value(
                Settings.user.value(UserSettings.UI_SCALE_FACTOR, defaultValue=1.0, type=float)
            )
        )
        ui_scaling_layout.addWidget(ui_scaling_slider)
        ui_scaling_slider_label_layout = QHBoxLayout()
        ui_scaling_slider_label_layout.addWidget(QLabel(text='1'))
        ui_scaling_slider_label_layout.addWidget(
            QLabel(text='1.5', alignment=Qt.AlignHCenter))  # type: ignore[call-overload]
        ui_scaling_slider_label_layout.addWidget(
            QLabel(text='2', alignment=Qt.AlignRight))  # type: ignore[call-overload]
        ui_scaling_layout.addLayout(ui_scaling_slider_label_layout)
        self.base_layout.addLayout(ui_scaling_layout)

    @staticmethod
    def convert_scale_factor_to_slider_value(scale_factor: float) -> int:
        return int((scale_factor - 1) / 0.25)

    @staticmethod
    def convert_slider_value_to_scale_factor(slider_value: int) -> float:
        return slider_value * 0.25 + 1

    @Slot(bool)
    def setting_toggled(self, checked: bool) -> None:
        source_btn = self.sender()
        btn_id = source_btn.objectName()
        Settings.user.setValue(btn_id, checked)

    @Slot(PySlider)
    def setting_slider_changed(self, slider: PySlider) -> None:
        slider_id = slider.objectName()
        if slider_id == UserSettings.UI_SCALE_FACTOR:
            value = self.convert_slider_value_to_scale_factor(slider.value())
        else:
            value = slider.value()
        Settings.user.setValue(slider_id, value)
