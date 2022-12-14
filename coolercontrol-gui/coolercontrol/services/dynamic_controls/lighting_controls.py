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

import logging
from collections import defaultdict
from typing import TYPE_CHECKING

from PySide6.QtCore import Slot, Qt, QMargins, QObject, QEvent
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QLabel, QBoxLayout, QFrame, QSpacerItem

from coolercontrol.models.device import Device, DeviceType
from coolercontrol.models.lighting_device_control import LightingDeviceControl
from coolercontrol.models.lighting_mode import LightingMode, LightingModeType
from coolercontrol.models.lighting_mode_widgets import LightingModeWidgets
from coolercontrol.models.saved_lighting_settings import ModeSetting, ModeSettings
from coolercontrol.models.settings import Setting, LightingSettings
from coolercontrol.services.utils import ButtonUtils
from coolercontrol.settings import Settings, UserSettings
from coolercontrol.view.uis.controls.speed_control_style import SPEED_CONTROL_STYLE
from coolercontrol.view.uis.controls.ui_lighting_control import Ui_LightingControl
from coolercontrol.view.widgets import PySlider, PyToggle
from coolercontrol.view.widgets.color_button.color_button import ColorButton
from coolercontrol.view.widgets.plus_minus_buttons.plus_minus_button import PlusMinusButton
from coolercontrol.view_models.observer import Observer
from coolercontrol.view_models.subject import Subject

_SYNC_CHANNEL = 'sync'
_NONE_MODE = 'None'
_MAX_SUPPORTED_COLORS = 48  # this only works if there are no other options to be displayed, so normally 40 is the max

if TYPE_CHECKING:
    from coolercontrol.view_models.devices_view_model import DevicesViewModel

log = logging.getLogger(__name__)


class LightingControls(QWidget, Subject):

    def __init__(self, devices_view_model: DevicesViewModel) -> None:
        super().__init__()
        self.toggle_bg_color = Settings.theme["app_color"]["dark_two"]
        self.toggle_circle_color = Settings.theme["app_color"]["icon_color"]
        self.toggle_active_color = Settings.theme["app_color"]["context_color"]
        self._observers: list[Observer] = []
        self._devices_view_model = devices_view_model
        # if device types other than liquidctl have lighting controls, this needs to be refactored:
        self._device_channel_mode_widgets: dict[int, dict[str, dict[LightingMode, LightingModeWidgets]]] = defaultdict(
            lambda: defaultdict(dict))
        self._channel_button_lighting_controls: dict[str, LightingDeviceControl] = {}
        self._is_first_run_per_channel: dict[str, bool] = defaultdict(lambda: True)
        self.current_channel_button_settings: dict[str, Setting] = {}
        self.current_set_settings: tuple[int, Setting] | None = None
        self.subscribe(devices_view_model)

    def subscribe(self, observer: Observer) -> None:
        self._observers.append(observer)

    def unsubscribe(self, observer: Observer) -> None:
        self._observers.remove(observer)

    def notify_observers(self) -> None:
        for observer in self._observers:
            observer.notify_me(self)

    def eventFilter(self, watched: QObject, event: QEvent) -> bool:
        """Allows applying of settings by just clicking in the lighting window, like in the speed UI"""
        if event.type() == QEvent.MouseButtonRelease:
            channel_btn_id = watched.objectName()
            if self.current_channel_button_settings.get(channel_btn_id) is not None:
                log.debug('Lighting Controls Clicked from %s', channel_btn_id)
                self._set_current_settings(channel_btn_id)
                return True
        return False

    def create_lighting_control(self, channel_name: str, channel_button_id: str) -> QWidget:
        device_control_widget, lighting_control = self._setup_lighting_control_ui(channel_button_id)
        self._initialize_lighting_controls(lighting_control, channel_name, channel_button_id)
        self._channel_button_lighting_controls[channel_button_id] = LightingDeviceControl(
            control_widget=device_control_widget,
            control_ui=lighting_control
        )
        return device_control_widget

    def _setup_lighting_control_ui(self, channel_button_id: str) -> tuple[QWidget, Ui_LightingControl]:
        device_control_widget = QWidget()
        device_control_widget.setObjectName(f"device_control_{channel_button_id}")
        lighting_control = Ui_LightingControl()
        lighting_control.setupUi(device_control_widget)  # type: ignore
        lighting_control.mode_label.setText('MODE')
        device_control_widget.setStyleSheet(
            SPEED_CONTROL_STYLE.format(
                _radius=8,
                _color=Settings.theme["app_color"]["text_foreground"],
                _border_color=Settings.theme["app_color"]["text_foreground"],
                _bg_color=Settings.theme["app_color"]['dark_one'],
                _active_color=Settings.theme["app_color"]["context_color"],
                _selection_bg_color=Settings.theme["app_color"]["dark_three"]
            ))
        #   crazy trick for an annoying 'bug', haven't found a better way:
        lighting_control.mode_combo_box.view().parentWidget().setStyleSheet(
            f'background-color: {Settings.theme["app_color"]["dark_one"]};margin-top: 0; margin-bottom: 0;')
        lighting_control.content_widget.setStyleSheet('font-size: 14pt;')
        lighting_control.content_widget.setObjectName(channel_button_id)
        lighting_control.content_widget.installEventFilter(self)
        return device_control_widget, lighting_control

    def _initialize_lighting_controls(
            self,
            lighting_control: Ui_LightingControl,
            channel_name: str,
            channel_button_id: str
    ) -> None:
        lighting_control.lighting_control_box.setTitle(channel_name.capitalize())
        lighting_control.mode_combo_box.setObjectName(channel_button_id)
        lighting_control.mode_combo_box.clear()
        device_id, channel_name, device_type = ButtonUtils.extract_info_from_channel_btn_id(channel_button_id)
        associated_device: Device | None = next(
            (
                device for device in self._devices_view_model.devices
                if device.type == device_type and device.type_id == device_id
            ),
            None,
        )
        if associated_device is None:
            log.error('Device not found in lighting controls')
            return
        none_mode = LightingMode('none', _NONE_MODE, 0, 0, False, False, LightingModeType.NONE)
        none_widget = QWidget()
        none_widget.setObjectName(none_mode.name)
        lighting_control.controls_layout.addWidget(none_widget)
        self._device_channel_mode_widgets[associated_device.type_id][channel_name][none_mode] = \
            LightingModeWidgets(channel_button_id, none_widget)
        for lighting_mode in associated_device.info.channels[channel_name].lighting_modes:
            self._device_channel_mode_widgets[associated_device.type_id][channel_name][lighting_mode] = \
                self._create_widgets_for_mode(
                    channel_button_id, lighting_mode, associated_device, lighting_control
                )
        # todo: add custom lighting modes bases on devices, etc.
        for mode in self._device_channel_mode_widgets[associated_device.type_id][channel_name]:
            lighting_control.mode_combo_box.addItem(mode.frontend_name)
        lighting_control.mode_combo_box.currentTextChanged.connect(self._show_mode_control_widget)
        last_applied_lighting = Settings.get_lighting_mode_settings_for_channel(
            associated_device.name, associated_device.type_id, channel_name).last
        if last_applied_lighting is not None and last_applied_lighting[0].type != LightingModeType.NONE:
            mode, _ = last_applied_lighting
            lighting_control.mode_combo_box.setCurrentText(mode.frontend_name)
        else:
            # handles the case where current text is not changed and settings aren't triggered because it's None
            self._is_first_run_per_channel[channel_button_id] = False

    def _create_widgets_for_mode(
            self,
            channel_btn_id: str,
            lighting_mode: LightingMode,
            associated_device: Device,
            lighting_control: Ui_LightingControl
    ) -> LightingModeWidgets:
        lighting_speeds: list[str] = associated_device.info.lighting_speeds  # type: ignore
        mode_widget = QWidget()
        mode_widget.setObjectName(lighting_mode.name)
        mode_layout = QVBoxLayout(mode_widget)
        mode_layout.setAlignment(Qt.AlignTop | Qt.AlignHCenter)  # type: ignore
        speed_direction_layout = QHBoxLayout()
        mode_layout.addLayout(speed_direction_layout)
        lighting_widgets = LightingModeWidgets(channel_btn_id, mode_widget)
        _, channel_name, _ = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        mode_setting = Settings.get_lighting_mode_setting_for_mode(
            associated_device.name, associated_device.type_id, channel_name, lighting_mode)  # type: ignore
        if lighting_mode.speed_enabled and lighting_speeds:
            self._create_lighting_speed_layout(mode_setting, lighting_speeds, speed_direction_layout, lighting_widgets)
        if lighting_mode.backward_enabled:
            self._create_direction_toggle(mode_setting, speed_direction_layout, lighting_widgets)
        if lighting_mode.max_colors > 0:
            self._create_color_buttons_layout(mode_setting, lighting_mode, mode_layout, lighting_widgets)
        lighting_control.controls_layout.addWidget(mode_widget)
        mode_widget.hide()
        return lighting_widgets

    def _create_lighting_speed_layout(
            self,
            mode_setting: ModeSetting,
            lighting_speeds: list[str],
            speed_direction_layout: QBoxLayout,
            lighting_widgets: LightingModeWidgets
    ) -> None:
        number_of_speeds = len(lighting_speeds)
        speed_layout = QVBoxLayout()
        speed_layout.setAlignment(Qt.AlignTop | Qt.AlignCenter)  # type: ignore
        speed_label = QLabel(text='Speed')
        speed_label.setAlignment(Qt.AlignHCenter)  # type: ignore
        speed_layout.addWidget(speed_label)
        speed_layout.addItem(QSpacerItem(10, 10))
        speed_slider = PySlider(
            bg_color=self.toggle_bg_color,
            bg_color_hover=self.toggle_bg_color,
            handle_color=self.toggle_circle_color,
            handle_color_hover=self.toggle_active_color,
            handle_color_pressed=self.toggle_active_color,
            orientation=Qt.Orientation.Horizontal,
            tickInterval=1, singleStep=1, minimum=0, maximum=(number_of_speeds - 1)
        )
        speed_slider.setFixedWidth(250)
        if mode_setting.speed_slider_value is not None:
            current_value: int = mode_setting.speed_slider_value
        elif number_of_speeds == 1:
            current_value = 1
        elif number_of_speeds == 5:
            current_value = 2
        else:
            current_value = number_of_speeds // 2
        speed_slider.setValue(current_value)
        mode_setting.speed_slider_value = current_value
        # noinspection PyUnresolvedReferences
        speed_slider.valueChanged.connect(self._slider_adjusted)
        speed_slider.setTracking(False)  # valueChanged signal is only emitted on release
        speed_slider.setObjectName(lighting_widgets.channel_btn_id)
        lighting_widgets.speed = speed_slider
        lighting_widgets.mode_speeds = lighting_speeds
        speed_layout.addWidget(speed_slider)
        speed_slider_label_layout = QHBoxLayout()
        speed_slider_label_layout.addWidget(QLabel(text="slow"))
        speed_slider_label_layout.addWidget(
            QLabel(text="fast", alignment=Qt.AlignRight))  # type: ignore[call-overload]
        speed_layout.addLayout(speed_slider_label_layout)
        speed_direction_layout.addLayout(speed_layout)

    def _create_direction_toggle(
            self,
            mode_setting: ModeSetting,
            speed_direction_layout: QBoxLayout,
            lighting_widgets: LightingModeWidgets
    ) -> None:
        direction_layout = QVBoxLayout()
        direction_layout.setAlignment(Qt.AlignTop | Qt.AlignCenter)  # type: ignore
        direction_label = QLabel(text='Backwards')
        direction_label.setAlignment(Qt.AlignCenter)  # type: ignore
        direction_layout.addWidget(direction_label)
        direction_layout.addItem(QSpacerItem(10, 10))
        direction_toggle = PyToggle(
            bg_color=self.toggle_bg_color,
            circle_color=self.toggle_circle_color,
            active_color=self.toggle_active_color,
            checked=mode_setting.backwards
        )
        # noinspection PyUnresolvedReferences
        direction_toggle.clicked.connect(self._direction_toggled)
        direction_toggle.setObjectName(lighting_widgets.channel_btn_id)
        toggle_container = QHBoxLayout()
        toggle_container.addItem(QSpacerItem(200, 20))
        toggle_container.addWidget(direction_toggle)
        toggle_container.addItem(QSpacerItem(200, 20))
        lighting_widgets.backwards = direction_toggle
        direction_layout.addLayout(toggle_container)
        speed_direction_layout.addLayout(direction_layout)

    def _create_color_buttons_layout(
            self,
            mode_setting: ModeSetting,
            lighting_mode: LightingMode,
            mode_layout: QBoxLayout,
            lighting_widgets: LightingModeWidgets
    ) -> None:
        mode_layout.addWidget(self._h_line())
        colors_layout = QVBoxLayout()
        colors_layout.setAlignment(Qt.AlignHCenter)  # type: ignore
        colors_label = QLabel(text='Colors')
        colors_label.setAlignment(Qt.AlignHCenter)  # type: ignore
        colors_layout.addWidget(colors_label)
        colors_layout.addItem(QSpacerItem(10, 5))
        if lighting_mode.min_colors != lighting_mode.max_colors:
            self._add_more_less_color_buttons(colors_layout, lighting_widgets)
        shown_starting_colors: int = lighting_mode.min_colors \
            if mode_setting.active_colors is None else mode_setting.active_colors
        has_all_color_settings: bool = len(mode_setting.button_colors) == lighting_mode.max_colors
        if not has_all_color_settings:
            mode_setting.button_colors.clear()
        color_buttons_row_1 = QHBoxLayout()
        color_buttons_row_2 = QHBoxLayout()
        color_buttons_row_3 = QHBoxLayout()
        color_buttons_row_4 = QHBoxLayout()
        color_buttons_row_5 = QHBoxLayout()
        color_buttons_row_6 = QHBoxLayout()
        for index in range(lighting_mode.max_colors):
            if index >= _MAX_SUPPORTED_COLORS:
                break
            if has_all_color_settings:
                color_button = ColorButton(color=mode_setting.button_colors[index])
            else:
                color_button = ColorButton()
                mode_setting.button_colors.append(color_button.color_hex())
            color_button.setObjectName(lighting_widgets.channel_btn_id)
            color_button.color_changed.connect(self._color_changed)
            if index >= shown_starting_colors:
                color_button.hide()
            lighting_widgets.color_buttons.append(color_button)
            # currently, supporting up to 48 colors
            if index // 8 == 0:
                color_buttons_row_1.addWidget(color_button)
            elif index // 8 == 1:
                color_buttons_row_2.addWidget(color_button)
            elif index // 8 == 2:
                color_buttons_row_3.addWidget(color_button)
            elif index // 8 == 3:
                color_buttons_row_4.addWidget(color_button)
            elif index // 8 == 4:
                color_buttons_row_5.addWidget(color_button)
            elif index // 8 == 5:
                color_buttons_row_6.addWidget(color_button)
        lighting_widgets.active_colors = shown_starting_colors
        colors_layout.addLayout(color_buttons_row_1)
        colors_layout.addLayout(color_buttons_row_2)
        colors_layout.addLayout(color_buttons_row_3)
        colors_layout.addLayout(color_buttons_row_4)
        colors_layout.addLayout(color_buttons_row_5)
        colors_layout.addLayout(color_buttons_row_6)
        mode_layout.addLayout(colors_layout)

    def _add_more_less_color_buttons(self, colors_layout: QBoxLayout, lighting_widgets: LightingModeWidgets) -> None:
        more_less_centering_layout = QHBoxLayout()
        more_less_widget = QWidget()
        more_less_widget.setMaximumWidth(100)
        more_less_centering_layout.addStretch()
        more_less_centering_layout.addWidget(more_less_widget)
        more_less_centering_layout.addStretch()
        more_less_layout = QHBoxLayout(more_less_widget)
        less_colors_button = PlusMinusButton(
            color=self.toggle_active_color,
            bg_color=self.toggle_bg_color,
            bg_color_pressed=self.toggle_active_color,
            text='-'
        )
        # noinspection PyUnresolvedReferences
        less_colors_button.pressed.connect(lambda: self._less_colors_pressed(lighting_widgets.channel_btn_id))
        less_colors_button.setObjectName(lighting_widgets.channel_btn_id)
        more_colors_button = PlusMinusButton(
            color=self.toggle_active_color,
            bg_color=self.toggle_bg_color,
            bg_color_pressed=self.toggle_active_color,
            text='+'
        )
        # noinspection PyUnresolvedReferences
        more_colors_button.pressed.connect(lambda: self._more_colors_pressed(lighting_widgets.channel_btn_id))
        more_colors_button.setObjectName(lighting_widgets.channel_btn_id)
        more_less_layout.addWidget(less_colors_button)
        more_less_layout.addWidget(more_colors_button)
        colors_layout.addLayout(more_less_centering_layout)
        colors_layout.addItem(QSpacerItem(10, 5))

    @Slot()
    def _show_mode_control_widget(self, mode_name: str) -> None:
        channel_btn_id = self.sender().objectName()
        log.debug('Lighting Mode chosen:  %s from %s', mode_name, channel_btn_id)
        device_id, channel_name, _ = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        for lighting_mode, widgets in self._device_channel_mode_widgets[device_id][channel_name].items():
            if lighting_mode.frontend_name == mode_name:
                if widgets.mode.parent() is None:
                    self._channel_button_lighting_controls[channel_btn_id].control_ui.controls_layout.addWidget(
                        widgets.mode
                    )
                widgets.mode.show()
                self._set_current_settings(channel_btn_id, widgets, lighting_mode)
            else:
                widgets.mode.hide()

    @Slot()
    def _slider_adjusted(self, speed: int) -> None:
        channel_btn_id = self.sender().objectName()
        log.debug('Lighting Slider adjusted:  %s from %s', speed, channel_btn_id)
        self._set_current_settings(channel_btn_id)

    @Slot()
    def _direction_toggled(self, checked: bool) -> None:
        channel_btn_id = self.sender().objectName()
        log.debug('Lighting Direction toggled:  %s from %s', checked, channel_btn_id)
        self._set_current_settings(channel_btn_id)

    @Slot()
    def _color_changed(self, color: str) -> None:
        channel_btn_id = self.sender().objectName()
        log.debug('Color Button toggled:  %s from %s', color, channel_btn_id)
        self._set_current_settings(channel_btn_id)

    @Slot()
    def _less_colors_pressed(self, channel_button_id: str | None) -> None:
        if channel_button_id is None:
            channel_btn_id = self.sender().objectName()
        else:
            channel_btn_id = channel_button_id
        log.debug('Less Colors Button pressed')
        device_id, channel_name, _ = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        for lighting_mode, lighting_widgets in self._device_channel_mode_widgets[device_id][channel_name].items():
            if lighting_mode.name == self.current_channel_button_settings[channel_btn_id].lighting_mode.name:
                if lighting_widgets.active_colors <= lighting_mode.min_colors:
                    break
                lighting_widgets.active_colors -= 1
                for index, color_btn in enumerate(lighting_widgets.color_buttons):
                    if index < lighting_widgets.active_colors:
                        color_btn.show()
                    else:
                        color_btn.hide()
                self._set_current_settings(channel_btn_id, lighting_widgets)
                break

    @Slot()
    def _more_colors_pressed(self, channel_button_id: str | None) -> None:
        if channel_button_id is None:
            channel_btn_id = self.sender().objectName()
        else:
            channel_btn_id = channel_button_id
        log.debug('More Colors Button pressed')
        device_id, channel_name, _ = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        for lighting_mode, lighting_widgets in self._device_channel_mode_widgets[device_id][channel_name].items():
            if lighting_mode.name == self.current_channel_button_settings[channel_btn_id].lighting_mode.name:
                if lighting_widgets.active_colors >= lighting_mode.max_colors:
                    break
                lighting_widgets.active_colors += 1
                for index, color_btn in enumerate(lighting_widgets.color_buttons):
                    if index < lighting_widgets.active_colors:
                        color_btn.show()
                    else:
                        color_btn.hide()
                self._set_current_settings(channel_btn_id, lighting_widgets)
                break

    @staticmethod
    def _h_line() -> QFrame:
        return QFrame(  # type: ignore[call-arg]
            frameShape=QFrame.HLine, frameShadow=QFrame.Plain, minimumHeight=30, contentsMargins=QMargins(40, 0, 40, 0)
        )

    def _set_current_settings(
            self, channel_btn_id: str,
            widgets: LightingModeWidgets | None = None,
            mode: LightingMode | None = None
    ) -> None:
        device_id, channel_name, device_type = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        associated_device: Device | None = next(
            (
                device for device in self._devices_view_model.devices
                if device.type == device_type and device.type_id == device_id
            ),
            None,
        )
        if associated_device is None:
            log.error('Device not found in lighting controls')
            return
        settings = Settings.get_lighting_mode_settings_for_channel(
            associated_device.name, associated_device.type_id, channel_name)  # type: ignore
        if mode is not None:
            self.current_channel_button_settings[channel_btn_id] = Setting(
                channel_name, lighting=LightingSettings(mode.name), lighting_mode=mode
            )
        current_mode = self.current_channel_button_settings[channel_btn_id].lighting_mode
        mode_setting = settings.all[current_mode]  # type: ignore
        if widgets is None:
            for lighting_mode, lighting_widgets in self._device_channel_mode_widgets[device_id][channel_name].items():
                if lighting_mode.name == self.current_channel_button_settings[channel_btn_id].lighting_mode.name:
                    widgets = lighting_widgets
                    break
            else:
                log.error('Mode not found in Lighting Mode Widgets')
                return
        if widgets.mode_speeds and widgets.speed is not None:
            speed_name = widgets.mode_speeds[widgets.speed.value()]
            self.current_channel_button_settings[channel_btn_id].lighting.speed = speed_name  # type: ignore
            mode_setting.speed_slider_value = widgets.speed.value()
        if widgets.backwards is not None:
            self.current_channel_button_settings[channel_btn_id].lighting.backward = widgets.backwards.isChecked()
            mode_setting.backwards = widgets.backwards.isChecked()
        if widgets.active_colors and widgets.color_buttons:
            self.current_channel_button_settings[channel_btn_id].lighting.colors.clear()  # type: ignore
            mode_setting.active_colors = widgets.active_colors
            for index, button in enumerate(widgets.color_buttons):
                if index >= widgets.active_colors:
                    break
                self.current_channel_button_settings[channel_btn_id].lighting.colors.append(button.color_rgb())
                mode_setting.button_colors[index] = button.color_hex()
        self._handle_sync_channels(device_type, device_id, channel_name, current_mode)  # type: ignore
        self.current_set_settings = device_id, self.current_channel_button_settings[channel_btn_id]
        log.debug(
            'Current settings for btn: %s : %s', channel_btn_id, self.current_channel_button_settings[channel_btn_id]
        )
        if self._should_apply_settings(settings, channel_btn_id):
            settings.last = current_mode, mode_setting  # type: ignore
            self.notify_observers()

    def _should_apply_settings(self, settings: ModeSettings, channel_btn_id: str) -> bool:
        """The first apply needs to be handled specially depending on settings"""
        if self._is_first_run_per_channel[channel_btn_id]:
            self._is_first_run_per_channel[channel_btn_id] = False
            return False
        return True

    def _handle_sync_channels(
            self, device_type: DeviceType, device_id: int, channel_name: str, current_mode: LightingMode
    ) -> None:
        """This makes sure that when setting sync channel, that the other channels are set to none and vise versa"""
        if current_mode.frontend_name != _NONE_MODE:
            for channel_button_id, device_control in self._channel_button_lighting_controls.items():
                found_device_id, found_channel_name, found_device_type = ButtonUtils.extract_info_from_channel_btn_id(
                    channel_button_id
                )
                if found_device_type == device_type and found_device_id == device_id and (
                        (channel_name == _SYNC_CHANNEL and found_channel_name != _SYNC_CHANNEL)
                        or (channel_name != _SYNC_CHANNEL and found_channel_name == _SYNC_CHANNEL)
                ):
                    device_control.control_ui.mode_combo_box.setCurrentText(_NONE_MODE)
