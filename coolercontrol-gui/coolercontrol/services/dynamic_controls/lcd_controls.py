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
from pathlib import Path
from typing import TYPE_CHECKING

from PySide6.QtCore import Slot, Qt, QMargins, QObject, QEvent
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QLabel, QBoxLayout, QFrame, QSpacerItem

from coolercontrol.models.device import Device
from coolercontrol.models.lcd_mode import LcdMode, LcdModeType
from coolercontrol.models.lighting_device_control import LightingDeviceControl
from coolercontrol.models.lighting_mode_widgets import LightingModeWidgets
from coolercontrol.models.saved_lcd_settings import LcdModeSetting, LcdModeSettings
from coolercontrol.models.settings import Setting, LcdSettings
from coolercontrol.services.utils import ButtonUtils
from coolercontrol.settings import Settings, UserSettings
from coolercontrol.view.uis.controls.speed_control_style import SPEED_CONTROL_STYLE
from coolercontrol.view.uis.controls.ui_lighting_control import Ui_LightingControl
from coolercontrol.view.widgets import PySlider
from coolercontrol.view.widgets.color_button.color_button import ColorButton
from coolercontrol.view.widgets.image_chooser_button.image_chooser_button import ImageChooserButton
from coolercontrol.view.widgets.plus_minus_buttons.plus_minus_button import PlusMinusButton
from coolercontrol.view_models.observer import Observer
from coolercontrol.view_models.subject import Subject

_SYNC_CHANNEL: str = "sync"
_NONE_MODE: str = "None"
# this only works if there are no other options to be displayed, so normally 40 is the max:
_MAX_SUPPORTED_COLORS: int = 48
_LCD_CHANNEL_NAME: str = "lcd"

if TYPE_CHECKING:
    from coolercontrol.view_models.devices_view_model import DevicesViewModel

log = logging.getLogger(__name__)


class LcdControls(QWidget, Subject):

    def __init__(self, devices_view_model: DevicesViewModel) -> None:
        super().__init__()
        self.toggle_bg_color = Settings.theme["app_color"]["dark_two"]
        self.toggle_circle_color = Settings.theme["app_color"]["icon_color"]
        self.toggle_active_color = Settings.theme["app_color"]["context_color"]
        self._observers: list[Observer] = []
        self._devices_view_model = devices_view_model
        self._device_channel_mode_widgets: dict[int, dict[str, dict[LcdMode, LightingModeWidgets]]] = defaultdict(
            lambda: defaultdict(dict))
        self._channel_button_lcd_controls: dict[str, LightingDeviceControl] = {}
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
        """Allows applying of settings by just clicking in the lcd window, like in the speed UI"""
        if event.type() == QEvent.MouseButtonRelease:
            channel_btn_id = watched.objectName()
            if self.current_channel_button_settings.get(channel_btn_id) is not None:
                log.debug("LCD Controls Clicked from %s", channel_btn_id)
                self._set_current_settings(channel_btn_id)
                return True
        return False

    def create_lcd_control(self, channel_button_id: str) -> QWidget:
        device_control_widget, lcd_control = self._setup_lcd_control_ui(channel_button_id)
        self._initialize_lcd_controls(lcd_control, _LCD_CHANNEL_NAME, channel_button_id)
        self._channel_button_lcd_controls[channel_button_id] = LightingDeviceControl(
            control_widget=device_control_widget,
            control_ui=lcd_control
        )
        return device_control_widget

    def _setup_lcd_control_ui(self, channel_button_id: str) -> tuple[QWidget, Ui_LightingControl]:
        """This used the lighting control as a base, as the needed base UI elements are the same"""
        device_control_widget = QWidget()
        device_control_widget.setObjectName(f"device_control_{channel_button_id}")
        lighting_control = Ui_LightingControl()
        lighting_control.setupUi(device_control_widget)  # type: ignore
        lighting_control.mode_label.setText("MODE")
        device_control_widget.setStyleSheet(
            SPEED_CONTROL_STYLE.format(
                _radius=8,
                _color=Settings.theme["app_color"]["text_foreground"],
                _border_color=Settings.theme["app_color"]["text_foreground"],
                _bg_color=Settings.theme["app_color"]["dark_one"],
                _active_color=Settings.theme["app_color"]["context_color"],
                _selection_bg_color=Settings.theme["app_color"]["dark_three"]
            ))
        #   crazy trick for an annoying 'bug', haven't found a better way:
        lighting_control.mode_combo_box.view().parentWidget().setStyleSheet(
            f'background-color: {Settings.theme["app_color"]["dark_one"]};margin-top: 0; margin-bottom: 0;')
        lighting_control.content_widget.setStyleSheet("font-size: 14pt;")
        lighting_control.content_widget.setObjectName(channel_button_id)
        lighting_control.content_widget.installEventFilter(self)
        return device_control_widget, lighting_control

    def _initialize_lcd_controls(
            self,
            lcd_control: Ui_LightingControl,
            channel_name: str,
            channel_button_id: str
    ) -> None:
        lcd_control.lighting_control_box.setTitle(channel_name.upper())
        lcd_control.mode_combo_box.setObjectName(channel_button_id)
        lcd_control.mode_combo_box.clear()
        device_id, channel_name, device_type = ButtonUtils.extract_info_from_channel_btn_id(channel_button_id)
        associated_device: Device | None = next(
            (
                device for device in self._devices_view_model.devices
                if device.type == device_type and device.type_id == device_id
            ),
            None,
        )
        if associated_device is None:
            log.error("Device not found in LCD controls for button: %s", channel_button_id)
            return
        none_mode = LcdMode(_NONE_MODE.lower(), _NONE_MODE, False, False, type=LcdModeType.NONE)
        none_widget = QWidget()
        none_widget.setObjectName(none_mode.name)
        lcd_control.controls_layout.addWidget(none_widget)
        self._device_channel_mode_widgets[associated_device.type_id][channel_name][none_mode] = \
            LightingModeWidgets(channel_button_id, none_widget)
        for lcd_mode in associated_device.info.channels[channel_name].lcd_modes:
            self._device_channel_mode_widgets[associated_device.type_id][channel_name][lcd_mode] = \
                self._create_widgets_for_mode(
                    channel_button_id, lcd_mode, associated_device, lcd_control
                )
        for mode in self._device_channel_mode_widgets[associated_device.type_id][channel_name]:
            lcd_control.mode_combo_box.addItem(mode.frontend_name)
        lcd_control.mode_combo_box.currentTextChanged.connect(self._show_mode_control_widget)
        last_applied_lcd: tuple[LcdMode, LcdModeSetting] = Settings.get_lcd_mode_settings_for_channel(
            associated_device.name, associated_device.type_id, channel_name).last
        if last_applied_lcd is not None and last_applied_lcd[0].type != LcdModeType.NONE:
            mode, _ = last_applied_lcd
            lcd_control.mode_combo_box.setCurrentText(mode.frontend_name)
        else:
            # handles the case where current text is not changed and settings aren't triggered because it's None
            self._is_first_run_per_channel[channel_button_id] = False

    def _create_widgets_for_mode(
            self,
            channel_btn_id: str,
            lcd_mode: LcdMode,
            associated_device: Device,
            lighting_control: Ui_LightingControl
    ) -> LightingModeWidgets:
        mode_widget = QWidget()
        mode_widget.setObjectName(lcd_mode.name)
        mode_layout = QVBoxLayout(mode_widget)
        mode_layout.setAlignment(Qt.AlignTop | Qt.AlignHCenter)  # type: ignore
        brightness_orientation_layout = QHBoxLayout()
        mode_layout.addLayout(brightness_orientation_layout)
        lighting_widgets = LightingModeWidgets(channel_btn_id, mode_widget)
        _, channel_name, _ = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        mode_setting = Settings.get_lcd_mode_setting_for_mode(
            associated_device.name, associated_device.type_id, channel_name, lcd_mode)  # type: ignore
        if lcd_mode.brightness:
            self._create_brightness_slider(mode_setting, brightness_orientation_layout, lighting_widgets)
        if lcd_mode.orientation:
            self._create_orientation_slider(mode_setting, brightness_orientation_layout, lighting_widgets)
        if lcd_mode.image:
            self._create_image_file_chooser(mode_setting, mode_layout, lighting_widgets)
        if lcd_mode.colors_max > 0:
            self._create_color_buttons_layout(mode_setting, lcd_mode, mode_layout, lighting_widgets)
        lighting_control.controls_layout.addWidget(mode_widget)
        mode_widget.hide()
        return lighting_widgets

    def _create_brightness_slider(
            self,
            mode_setting: LcdModeSetting,
            brightness_orientation_layout: QBoxLayout,
            lighting_widgets: LightingModeWidgets
    ) -> None:
        brightness_layout = QVBoxLayout()
        brightness_layout.setAlignment(Qt.AlignTop | Qt.AlignCenter)  # type: ignore
        brightness_label = QLabel(text="Brightness")
        brightness_label.setAlignment(Qt.AlignHCenter)  # type: ignore
        brightness_layout.addWidget(brightness_label)
        brightness_layout.addItem(QSpacerItem(10, 10))
        brightness_slider = PySlider(
            bg_color=self.toggle_bg_color,
            bg_color_hover=self.toggle_bg_color,
            handle_color=self.toggle_circle_color,
            handle_color_hover=self.toggle_active_color,
            handle_color_pressed=self.toggle_active_color,
            orientation=Qt.Orientation.Horizontal,
            # for whatever reason I cannot get singleStep to work properly (singleStep=10) Working around this
            tickInterval=1, singleStep=1, minimum=0, maximum=10
        )
        brightness_slider.setFixedWidth(250)
        if mode_setting.brightness_slider_value is not None:
            current_value: int = mode_setting.brightness_slider_value
        else:
            current_value = 5
        brightness_slider.setValue(current_value)
        mode_setting.brightness_slider_value = current_value
        # noinspection PyUnresolvedReferences
        brightness_slider.valueChanged.connect(self._brightness_slider_adjusted)
        brightness_slider.setTracking(False)  # valueChanged signal is only emitted on release
        brightness_slider.setObjectName(lighting_widgets.channel_btn_id)
        lighting_widgets.brightness = brightness_slider
        brightness_layout.addWidget(brightness_slider)
        brightness_slider_label_layout = QHBoxLayout()
        brightness_slider_label_layout.addWidget(QLabel(text="0%"))
        brightness_slider_label_layout.addWidget(
            QLabel(text="50%", alignment=Qt.AlignHCenter))  # type: ignore[call-overload]
        brightness_slider_label_layout.addWidget(
            QLabel(text="100%", alignment=Qt.AlignRight))  # type: ignore[call-overload]
        brightness_layout.addLayout(brightness_slider_label_layout)
        brightness_orientation_layout.addLayout(brightness_layout)

    def _create_orientation_slider(
            self,
            mode_setting: LcdModeSetting,
            brightness_orientation_layout: QBoxLayout,
            lighting_widgets: LightingModeWidgets
    ) -> None:
        orientation_layout = QVBoxLayout()
        orientation_layout.setAlignment(Qt.AlignTop | Qt.AlignCenter)  # type: ignore
        orientation_label = QLabel(text="Orientation")
        orientation_label.setAlignment(Qt.AlignHCenter)  # type: ignore
        orientation_layout.addWidget(orientation_label)
        orientation_layout.addItem(QSpacerItem(10, 10))
        orientation_slider = PySlider(
            bg_color=self.toggle_bg_color,
            bg_color_hover=self.toggle_bg_color,
            handle_color=self.toggle_circle_color,
            handle_color_hover=self.toggle_active_color,
            handle_color_pressed=self.toggle_active_color,
            orientation=Qt.Orientation.Horizontal,
            tickInterval=1, singleStep=1, minimum=0, maximum=3
        )
        orientation_slider.setFixedWidth(250)
        if mode_setting.orientation_slider_value is not None:
            current_value: int = mode_setting.orientation_slider_value
        else:
            current_value = 0
        orientation_slider.setValue(current_value)
        mode_setting.orientation_slider_value = current_value
        # noinspection PyUnresolvedReferences
        orientation_slider.valueChanged.connect(self._orientation_slider_adjusted)
        orientation_slider.setTracking(False)  # valueChanged signal is only emitted on release
        orientation_slider.setObjectName(lighting_widgets.channel_btn_id)
        lighting_widgets.orientation = orientation_slider
        orientation_layout.addWidget(orientation_slider)
        orientation_slider_label_layout = QHBoxLayout()
        orientation_slider_label_layout.addWidget(QLabel(text="0°"))
        orientation_slider_label_layout.addWidget(
            QLabel(text="90°        180°", alignment=Qt.AlignHCenter))  # type: ignore[call-overload]
        orientation_slider_label_layout.addWidget(
            QLabel(text="270°", alignment=Qt.AlignRight))  # type: ignore[call-overload]
        orientation_layout.addLayout(orientation_slider_label_layout)
        brightness_orientation_layout.addLayout(orientation_layout)

    def _create_image_file_chooser(
            self,
            mode_setting: LcdModeSetting,
            mode_layout: QBoxLayout,
            lighting_widgets: LightingModeWidgets
    ) -> None:
        file_chooser_layout = QVBoxLayout()
        file_chooser_layout.setAlignment(Qt.AlignHCenter)
        file_chooser_layout.addItem(QSpacerItem(10, 10))
        image_path = Path(mode_setting.image_file_src) if mode_setting.image_file_src is not None else None
        button = ImageChooserButton(
            color=Settings.theme["app_color"]["text_foreground"],
            bg_color=Settings.theme["app_color"]["dark_one"],
            bg_color_hover=Settings.theme["app_color"]["dark_three"],
            bg_color_pressed=Settings.theme["app_color"]["context_color"],
            image=image_path,
            parent=self
        )
        button.setObjectName(lighting_widgets.channel_btn_id)
        button.image_changed.connect(self._image_changed)
        lighting_widgets.file_picker = button
        file_chooser_layout.addWidget(button)
        mode_layout.addLayout(file_chooser_layout)

    def _create_color_buttons_layout(
            self,
            mode_setting: LcdModeSetting,
            lcd_mode: LcdMode,
            mode_layout: QBoxLayout,
            lighting_widgets: LightingModeWidgets
    ) -> None:
        mode_layout.addWidget(self._h_line())
        colors_layout = QVBoxLayout()
        colors_layout.setAlignment(Qt.AlignHCenter)  # type: ignore
        colors_label = QLabel(text="Colors")
        colors_label.setAlignment(Qt.AlignHCenter)  # type: ignore
        colors_layout.addWidget(colors_label)
        colors_layout.addItem(QSpacerItem(10, 5))
        if lcd_mode.colors_min != lcd_mode.colors_max:
            self._add_more_less_color_buttons(colors_layout, lighting_widgets)
        shown_starting_colors: int = lcd_mode.colors_min \
            if mode_setting.active_colors is None else mode_setting.active_colors
        has_all_color_settings: bool = len(mode_setting.button_colors) == lcd_mode.colors_max
        if not has_all_color_settings:
            mode_setting.button_colors.clear()
        color_buttons_row_1 = QHBoxLayout()
        color_buttons_row_2 = QHBoxLayout()
        color_buttons_row_3 = QHBoxLayout()
        color_buttons_row_4 = QHBoxLayout()
        color_buttons_row_5 = QHBoxLayout()
        color_buttons_row_6 = QHBoxLayout()
        for index in range(lcd_mode.colors_max):
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
            text="-"
        )
        # noinspection PyUnresolvedReferences
        less_colors_button.pressed.connect(lambda: self._less_colors_pressed(lighting_widgets.channel_btn_id))
        less_colors_button.setObjectName(lighting_widgets.channel_btn_id)
        more_colors_button = PlusMinusButton(
            color=self.toggle_active_color,
            bg_color=self.toggle_bg_color,
            bg_color_pressed=self.toggle_active_color,
            text="+"
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
        log.debug("LCD Mode chosen:  %s from %s", mode_name, channel_btn_id)
        device_id, channel_name, _ = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        for lcd_mode, widgets in self._device_channel_mode_widgets[device_id][channel_name].items():
            if lcd_mode.frontend_name == mode_name:
                if widgets.mode.parent() is None:
                    self._channel_button_lcd_controls[channel_btn_id].control_ui.controls_layout.addWidget(
                        widgets.mode
                    )
                widgets.mode.show()
                self._set_current_settings(channel_btn_id, widgets, lcd_mode)
            else:
                widgets.mode.hide()

    @Slot()
    def _brightness_slider_adjusted(self, brightness: int) -> None:
        channel_btn_id = self.sender().objectName()
        log.debug("Brightness Slider adjusted:  %s from %s", brightness, channel_btn_id)
        self._set_current_settings(channel_btn_id)

    @Slot()
    def _orientation_slider_adjusted(self, orientation: int) -> None:
        channel_btn_id = self.sender().objectName()
        log.debug("Brightness Slider adjusted:  %s from %s", orientation, channel_btn_id)
        self._set_current_settings(channel_btn_id)

    @Slot()
    def _image_changed(self, image_path: Path | None) -> None:
        channel_btn_id = self.sender().objectName()
        log.debug("LCD Image File Button clicked toggled. File %s from %s", image_path, channel_btn_id)
        self._set_current_settings(channel_btn_id)

    @Slot()
    def _color_changed(self, color: str) -> None:
        channel_btn_id = self.sender().objectName()
        log.debug("Color Button toggled:  %s from %s", color, channel_btn_id)
        self._set_current_settings(channel_btn_id)

    @Slot()
    def _less_colors_pressed(self, channel_button_id: str | None) -> None:
        channel_btn_id = self.sender().objectName() if channel_button_id is None else channel_button_id
        log.debug("Less Colors Button pressed")
        device_id, channel_name, _ = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        for lcd_mode, lighting_widgets in self._device_channel_mode_widgets[device_id][channel_name].items():
            if lcd_mode.name == self.current_channel_button_settings[channel_btn_id].lcd_mode.name:
                if lighting_widgets.active_colors <= lcd_mode.colors_min:
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
        channel_btn_id = self.sender().objectName() if channel_button_id is None else channel_button_id
        log.debug("More Colors Button pressed")
        device_id, channel_name, _ = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        for lcd_mode, lighting_widgets in self._device_channel_mode_widgets[device_id][channel_name].items():
            if lcd_mode.name == self.current_channel_button_settings[channel_btn_id].lcd_mode.name:
                if lighting_widgets.active_colors >= lcd_mode.colors_max:
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
            mode: LcdMode | None = None,
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
            log.error("Device not found in LCD controls")
            return
        settings: LcdModeSettings = Settings.get_lcd_mode_settings_for_channel(
            associated_device.name, associated_device.type_id, channel_name)  # type: ignore
        if mode is not None:
            self.current_channel_button_settings[channel_btn_id] = Setting(
                channel_name, lcd=LcdSettings(mode.name), lcd_mode=mode
            )
        current_mode = self.current_channel_button_settings[channel_btn_id].lcd_mode
        mode_setting: LcdModeSetting = settings.all[current_mode]
        if widgets is None:
            for lcd_mode, lighting_widgets in self._device_channel_mode_widgets[device_id][channel_name].items():
                if lcd_mode.name == self.current_channel_button_settings[channel_btn_id].lcd_mode.name:
                    widgets = lighting_widgets
                    break
            else:
                log.error("Mode not found in LCD Mode Widgets")
                return
        if widgets.brightness is not None:
            current_brightness_value = widgets.brightness.value()
            self.current_channel_button_settings[channel_btn_id].lcd.brightness = current_brightness_value * 10
            mode_setting.brightness_slider_value = current_brightness_value
        if widgets.orientation is not None:
            current_orientation_slider_value = widgets.orientation.value()
            self.current_channel_button_settings[channel_btn_id].lcd.orientation = current_orientation_slider_value * 90
            mode_setting.orientation_slider_value = current_orientation_slider_value
        if widgets.file_picker is not None:
            image_file_src: str | None = str(widgets.file_picker.image_path) \
                if widgets.file_picker.image_path is not None else None
            image_file_processed: str | None = str(widgets.file_picker.image_path_processed) \
                if widgets.file_picker.image_path_processed is not None else None
            self.current_channel_button_settings[channel_btn_id].lcd.image_file_src = image_file_src
            self.current_channel_button_settings[channel_btn_id].lcd.image_file_processed = image_file_processed
            mode_setting.image_file_src = image_file_src
            mode_setting.image_file_processed = image_file_processed
        if widgets.active_colors and widgets.color_buttons:
            self.current_channel_button_settings[channel_btn_id].lcd.colors.clear()  # type: ignore
            mode_setting.active_colors = widgets.active_colors
            for index, button in enumerate(widgets.color_buttons):
                if index >= widgets.active_colors:
                    break
                self.current_channel_button_settings[channel_btn_id].lcd.colors.append(button.color_rgb())
                mode_setting.button_colors[index] = button.color_hex()
        self.current_set_settings = device_id, self.current_channel_button_settings[channel_btn_id]
        log.debug(
            "Current settings for btn: %s : %s", channel_btn_id, self.current_channel_button_settings[channel_btn_id]
        )
        if self._should_apply_settings(settings, channel_btn_id):
            settings.last = current_mode, mode_setting  # type: ignore
            self.notify_observers()

    def _should_apply_settings(self, settings: LcdModeSettings, channel_btn_id: str) -> bool:
        """The first apply needs to be handled specially depending on settings"""
        if self._is_first_run_per_channel[channel_btn_id]:
            self._is_first_run_per_channel[channel_btn_id] = False
            return False
        return True
