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
from PySide6.QtWidgets import QVBoxLayout, QHBoxLayout, QLabel, QFrame, QSpacerItem, QScrollArea

from coolero.settings import Settings, UserSettings, FeatureToggle, IS_APP_IMAGE
from coolero.view.uis.windows.main_window.scroll_area_style import SCROLL_AREA_STYLE
from coolero.view.widgets import PyToggle, PySlider


class SettingsPage(QScrollArea):

    def __init__(self) -> None:
        super().__init__()
        self.theme: Dict = Settings.theme
        self.setStyleSheet(
            SCROLL_AREA_STYLE.format(
                _scroll_bar_bg_color=Settings.theme["app_color"]["bg_one"],
                _scroll_bar_btn_color=Settings.theme["app_color"]["dark_four"],
                _context_color=Settings.theme["app_color"]["context_color"],
                _bg_color=Settings.theme["app_color"]["bg_one"]
            ) + f';font: 14pt; background: {Settings.theme["app_color"]["bg_two"]};'
        )
        self.base_layout = QVBoxLayout(self)
        inner_frame_widget = QFrame(self)
        inner_frame_widget.setLayout(self.base_layout)
        self.setWidgetResizable(True)
        self.setWidget(inner_frame_widget)

        self.toggle_bg_color = self.theme["app_color"]["dark_two"]
        self.toggle_circle_color = self.theme["app_color"]["icon_color"]
        self.toggle_active_color = self.theme["app_color"]["context_color"]

        self.setting_save_window_size()
        self.base_layout.addItem(self.spacer())
        self.setting_hide_on_close()
        self.base_layout.addItem(self.spacer())
        self.setting_hide_on_minimize()
        self.base_layout.addItem(self.spacer())
        self.setting_start_minimized()
        self.base_layout.addItem(self.spacer())
        self.setting_confirm_exit()
        if IS_APP_IMAGE or FeatureToggle.testing:
            self.base_layout.addItem(self.spacer())
            self.setting_check_for_updates()
        self.base_layout.addItem(self.spacer())
        self.setting_desktop_notifications()
        self.base_layout.addItem(self.spacer())
        self.setting_load_applied_at_startup()

        self.base_layout.addWidget(self.line())  # app restart required settings are below this line

        self.setting_enable_light_theme()
        self.base_layout.addItem(self.spacer())
        self.setting_enable_light_tray_icon()
        self.base_layout.addItem(self.spacer())
        self.setting_enable_overview_smoothing()
        self.base_layout.addItem(self.spacer())
        self.setting_enable_hwmon()
        self.base_layout.addItem(self.spacer())
        self.setting_enable_hwmon_filter()
        self.base_layout.addItem(self.spacer())
        self.setting_enable_hwmon_temps()
        self.base_layout.addItem(self.spacer())
        self.setting_ui_scaling()

        self.notes_layout = QVBoxLayout()
        self.notes_layout.setAlignment(Qt.AlignBottom)
        self.requires_restart_label = QLabel()
        self.requires_restart_label.setTextFormat(Qt.TextFormat.RichText)
        self.requires_restart_label.setText('<i><b>*</b> requires restart</i>')
        self.requires_restart_label.setAlignment(Qt.AlignRight)
        self.notes_layout.addWidget(self.requires_restart_label)
        self.experimental_label = QLabel()
        self.experimental_label.setTextFormat(Qt.TextFormat.RichText)
        self.experimental_label.setText('<i><b>**</b> experimental</i>')
        self.experimental_label.setAlignment(Qt.AlignRight)
        self.notes_layout.addWidget(self.experimental_label)
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
        save_window_size_label = QLabel(text='Save Window State on Exit')
        save_window_size_label.setToolTip('Save the application window size and position')
        save_window_size_layout.addWidget(save_window_size_label)
        save_window_size_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.SAVE_WINDOW_SIZE, defaultValue=True, type=bool)
        )
        save_window_size_toggle.setObjectName(UserSettings.SAVE_WINDOW_SIZE)
        save_window_size_toggle.clicked.connect(self.setting_toggled)
        save_window_size_layout.addWidget(save_window_size_toggle)
        self.base_layout.addLayout(save_window_size_layout)

    def setting_hide_on_close(self) -> None:
        hide_on_close_layout = QHBoxLayout()
        hide_on_close_label = QLabel(text='Close to Tray')
        hide_on_close_label.setToolTip('Leave the app running in the system tray when closing the window')
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

    def setting_hide_on_minimize(self) -> None:
        hide_on_minimize_layout = QHBoxLayout()
        hide_on_minimize_label = QLabel(text='Minimize to Tray')
        hide_on_minimize_label.setToolTip('On minimize the app will go to the system tray')
        hide_on_minimize_layout.addWidget(hide_on_minimize_label)
        hide_on_minimize_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.HIDE_ON_MINIMIZE, defaultValue=False, type=bool)
        )
        hide_on_minimize_toggle.setObjectName(UserSettings.HIDE_ON_MINIMIZE)
        hide_on_minimize_toggle.clicked.connect(self.setting_toggled)
        hide_on_minimize_layout.addWidget(hide_on_minimize_toggle)
        self.base_layout.addLayout(hide_on_minimize_layout)

    def setting_start_minimized(self) -> None:
        start_minimized_layout = QHBoxLayout()
        start_minimized_label = QLabel(text='Start minimized')
        start_minimized_label.setToolTip('Minimize the app on startup')
        start_minimized_layout.addWidget(start_minimized_label)
        start_minimized_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.START_MINIMIZED, defaultValue=False, type=bool)
        )
        start_minimized_toggle.setObjectName(UserSettings.START_MINIMIZED)
        start_minimized_toggle.clicked.connect(self.setting_toggled)
        start_minimized_layout.addWidget(start_minimized_toggle)
        self.base_layout.addLayout(start_minimized_layout)

    def setting_confirm_exit(self) -> None:
        confirm_exit_layout = QHBoxLayout()
        confirm_exit_label = QLabel(text='Confirm on Exit')
        confirm_exit_label.setToolTip('Ask for confirmation when quiting the application')
        confirm_exit_layout.addWidget(confirm_exit_label)
        confirm_exit_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.CONFIRM_EXIT, defaultValue=True, type=bool)
        )
        confirm_exit_toggle.setObjectName(UserSettings.CONFIRM_EXIT)
        confirm_exit_toggle.clicked.connect(self.setting_toggled)
        confirm_exit_layout.addWidget(confirm_exit_toggle)
        self.base_layout.addLayout(confirm_exit_layout)

    def setting_check_for_updates(self) -> None:
        check_for_updates_layout = QHBoxLayout()
        check_for_updates_label = QLabel(text='Check for updates at startup')
        check_for_updates_label.setToolTip('Check for AppImage updates at startup')
        check_for_updates_layout.addWidget(check_for_updates_label)
        check_for_updates_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.CHECK_FOR_UPDATES, defaultValue=False, type=bool)
        )
        check_for_updates_toggle.setObjectName(UserSettings.CHECK_FOR_UPDATES)
        check_for_updates_toggle.clicked.connect(self.setting_toggled)
        check_for_updates_layout.addWidget(check_for_updates_toggle)
        self.base_layout.addLayout(check_for_updates_layout)

    def setting_load_applied_at_startup(self) -> None:
        apply_at_startup_layout = QHBoxLayout()
        apply_at_startup_label = QLabel(text='Load applied profiles at startup')
        apply_at_startup_label.setToolTip('Loads the last applied profiles at startup')
        apply_at_startup_layout.addWidget(apply_at_startup_label)
        apply_at_startup_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.LOAD_APPLIED_AT_STARTUP, defaultValue=True, type=bool)
        )
        apply_at_startup_toggle.setObjectName(UserSettings.LOAD_APPLIED_AT_STARTUP)
        apply_at_startup_toggle.clicked.connect(self.setting_toggled)
        apply_at_startup_layout.addWidget(apply_at_startup_toggle)
        self.base_layout.addLayout(apply_at_startup_layout)

    def setting_desktop_notifications(self) -> None:
        desktop_notifications_layout = QHBoxLayout()
        desktop_notifications_label = QLabel(text='Desktop notifications')
        desktop_notifications_label.setToolTip('Enables desktop notifications')
        desktop_notifications_layout.addWidget(desktop_notifications_label)
        desktop_notifications_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.DESKTOP_NOTIFICATIONS, defaultValue=True, type=bool)
        )
        desktop_notifications_toggle.setObjectName(UserSettings.DESKTOP_NOTIFICATIONS)
        desktop_notifications_toggle.clicked.connect(self.setting_toggled)
        desktop_notifications_layout.addWidget(desktop_notifications_toggle)
        self.base_layout.addLayout(desktop_notifications_layout)

    def setting_enable_light_theme(self) -> None:
        enable_light_theme_layout = QHBoxLayout()
        enable_light_theme_label = QLabel(text='<b>*</b> Light Theme')
        enable_light_theme_label.setToolTip('Switch between the light and dark UI theme')
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

    def setting_enable_light_tray_icon(self) -> None:
        layout = QHBoxLayout()
        label = QLabel(text='<b>*</b> Brighter Tray Icon')
        label.setToolTip('Switch to a brighter tray icon for better visibility in dark themes')
        layout.addWidget(label)
        toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.ENABLE_BRIGHT_TRAY_ICON, defaultValue=False, type=bool)
        )
        toggle.setObjectName(UserSettings.ENABLE_BRIGHT_TRAY_ICON)
        toggle.clicked.connect(self.setting_toggled)
        layout.addWidget(toggle)
        self.base_layout.addLayout(layout)

    def setting_enable_overview_smoothing(self) -> None:
        enable_smoothing_layout = QHBoxLayout()
        enable_smoothing_label = QLabel(text='<b>*</b> Graph Smoothing')
        enable_smoothing_label.setToolTip(
            'Lightly smooth the graph for cpu and gpu data which can have rapid fluctuations')
        enable_smoothing_layout.addWidget(enable_smoothing_label)
        enable_smoothing_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.ENABLE_SMOOTHING, defaultValue=True, type=bool)
        )
        enable_smoothing_toggle.setObjectName(UserSettings.ENABLE_SMOOTHING)
        enable_smoothing_toggle.clicked.connect(self.setting_toggled)
        enable_smoothing_layout.addWidget(enable_smoothing_toggle)
        self.base_layout.addLayout(enable_smoothing_layout)

    def setting_enable_hwmon(self) -> None:
        enable_hwmon_layout = QHBoxLayout()
        enable_hwmon_label = QLabel(text='<b>*</b> Hwmon Support <b>**</b>')
        enable_hwmon_label.setToolTip('Enables experimental support for detected hwmon devices')
        enable_hwmon_layout.addWidget(enable_hwmon_label)
        enable_hwmon_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.ENABLE_HWMON, defaultValue=False, type=bool)
        )
        enable_hwmon_toggle.setObjectName(UserSettings.ENABLE_HWMON)
        enable_hwmon_toggle.clicked.connect(self.setting_toggled)
        enable_hwmon_layout.addWidget(enable_hwmon_toggle)
        self.base_layout.addLayout(enable_hwmon_layout)

    def setting_enable_hwmon_filter(self) -> None:
        enable_hwmon_filter_layout = QHBoxLayout()
        enable_hwmon_filter_label = QLabel(text='<b>*</b> Hwmon Filter')
        enable_hwmon_filter_label.setToolTip(
            'Filters detected hwmon sensors for a more reasonable list of sensors.'
        )
        enable_hwmon_filter_layout.addWidget(enable_hwmon_filter_label)
        enable_hwmon_filter_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.ENABLE_HWMON_FILTER, defaultValue=True, type=bool)
        )
        enable_hwmon_filter_toggle.setObjectName(UserSettings.ENABLE_HWMON_FILTER)
        enable_hwmon_filter_toggle.clicked.connect(self.setting_toggled)
        enable_hwmon_filter_layout.addWidget(enable_hwmon_filter_toggle)
        self.base_layout.addLayout(enable_hwmon_filter_layout)

    def setting_enable_hwmon_temps(self) -> None:
        enable_hwmon_temps_layout = QHBoxLayout()
        enable_hwmon_temps_label = QLabel(text='<b>*</b> Hwmon Temps')
        enable_hwmon_temps_label.setToolTip(
            'Enables the display and use of all Hwmon temperature sensors with reasonable values.'
        )
        enable_hwmon_temps_layout.addWidget(enable_hwmon_temps_label)
        enable_hwmon_temps_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=Settings.user.value(UserSettings.ENABLE_HWMON_TEMPS, defaultValue=False, type=bool)
        )
        enable_hwmon_temps_toggle.setObjectName(UserSettings.ENABLE_HWMON_TEMPS)
        enable_hwmon_temps_toggle.clicked.connect(self.setting_toggled)
        enable_hwmon_temps_layout.addWidget(enable_hwmon_temps_toggle)
        self.base_layout.addLayout(enable_hwmon_temps_layout)

    def setting_ui_scaling(self) -> None:
        ui_scaling_layout = QVBoxLayout()
        ui_scaling_layout.setAlignment(Qt.AlignTop)
        ui_scaling_label = QLabel(text='<b>*</b> UI Scaling Factor')
        ui_scaling_label.setToolTip('Manually set the UI scaling, mainly for HiDPI scaling')
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
        ui_scaling_slider_label_layout.addWidget(QLabel(text='1x'))
        ui_scaling_slider_label_layout.addWidget(
            QLabel(text='1.5x', alignment=Qt.AlignHCenter))  # type: ignore[call-overload]
        ui_scaling_slider_label_layout.addWidget(
            QLabel(text='2x', alignment=Qt.AlignRight))  # type: ignore[call-overload]
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
