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

from __future__ import annotations

import logging
from typing import List, Tuple, Dict, Optional, TYPE_CHECKING

from PySide6.QtCore import QObject
from PySide6.QtWidgets import QWidget

from models.device_control import DeviceControl
from models.device import Device, DeviceType
from models.speed_profile import SpeedProfile
from models.temp_source import TempSource
from services.utils import ButtonUtils
from view.uis.canvases.speed_control_canvas import SpeedControlCanvas
from view.uis.controls.speed_control_style import SPEED_CONTROL_STYLE
from view.uis.controls.ui_speed_control import Ui_SpeedControl
from view_models.devices_view_model import DevicesViewModel

_LOG = logging.getLogger(__name__)

if TYPE_CHECKING:
    from coolero import MainWindow  # type: ignore[attr-defined]


class DynamicControls(QObject):

    def __init__(self,
                 devices_view_model: DevicesViewModel,
                 main_window: MainWindow
                 ) -> None:
        super().__init__()
        self._devices_view_model = devices_view_model
        self._main_window = main_window
        self._channel_button_device_controls: Dict[str, DeviceControl] = {}

    def create_speed_control(self, channel_name: str, channel_button_id: str) -> QWidget:
        """Creates the speed control Widget for specific channel button"""
        device_control_widget, speed_control = self._setup_speed_control_ui(channel_button_id)
        temp_sources_and_profiles = self._initialize_speed_control_dynamic_properties(
            speed_control, channel_name, channel_button_id
        )
        self._channel_button_device_controls[channel_button_id] = DeviceControl(
            control_widget=device_control_widget,
            control_ui=speed_control,
            temp_sources_and_profiles=temp_sources_and_profiles
        )
        return device_control_widget

    def create_lighting_control(self, channel_name: str, channel_button_id: str) -> QWidget:
        device_control_widget = QWidget()
        device_control_widget.setObjectName(f"device_control_{channel_button_id}")
        device_control_widget.setStyleSheet(f'''
                QGroupBox {{
                    color: {self._main_window.theme["app_color"]["text_foreground"]};
                    font-size: 14pt;
                    border: 1px solid {self._main_window.theme["app_color"]["text_foreground"]};
                    border-radius: 6px;
                    margin-top: 14px;
                }}
                ''')
        # todo: ADD LIGHTING CHANNEL CONTROL UI !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
        return device_control_widget

    def _setup_speed_control_ui(self, channel_button_id: str) -> Tuple[QWidget, Ui_SpeedControl]:
        device_control_widget = QWidget()
        device_control_widget.setObjectName(f"device_control_{channel_button_id}")
        speed_control = Ui_SpeedControl()
        speed_control.setupUi(device_control_widget)
        device_control_widget.setStyleSheet(
            SPEED_CONTROL_STYLE.format(
                _radius=8,
                _color=self._main_window.theme["app_color"]["text_foreground"],
                _border_color=self._main_window.theme["app_color"]["text_foreground"],
                _bg_color=self._main_window.theme["app_color"]['dark_one'],
                _active_color=self._main_window.theme["app_color"]["context_color"],
                _selection_bg_color=self._main_window.theme["app_color"]["dark_three"]
            ))
        #   crazy trick for an annoying 'bug', haven't found a better way:
        speed_control.temp_combo_box.view().parentWidget().setStyleSheet(
            f'background-color: {self._main_window.theme["app_color"]["dark_one"]};margin-top: 0; margin-bottom: 0;')
        speed_control.profile_combo_box.view().parentWidget().setStyleSheet(
            f'background-color: {self._main_window.theme["app_color"]["dark_one"]};margin-top: 0; margin-bottom: 0;')
        speed_control.content_widget.setStyleSheet('font-size: 14pt;')
        return device_control_widget, speed_control

    def _initialize_speed_control_dynamic_properties(
            self,
            speed_control: Ui_SpeedControl,
            channel_name: str,
            channel_button_id: str
    ) -> Dict[TempSource, List[SpeedProfile]]:
        speed_control.speed_control_box.setTitle(channel_name.capitalize())
        speed_control.temp_combo_box.setObjectName(channel_button_id)
        speed_control.temp_combo_box.clear()
        speed_control.profile_combo_box.setObjectName(channel_button_id)
        speed_control.profile_combo_box.clear()
        temp_sources_and_profiles, device = self._device_temp_sources_and_profiles(channel_button_id)

        speed_control_graph_canvas = SpeedControlCanvas(
            device=device,
            channel_name=channel_name,
            starting_temp_source=next(iter(temp_sources_and_profiles.keys())),
            temp_sources=list(temp_sources_and_profiles.keys()),
            starting_speed_profile=next(iter(next(iter(temp_sources_and_profiles.values()))), SpeedProfile.NONE)
        )
        speed_control.graph_layout.addWidget(speed_control_graph_canvas)
        speed_control.temp_combo_box.currentTextChanged.connect(
            speed_control_graph_canvas.chosen_temp_source)
        speed_control.profile_combo_box.currentTextChanged.connect(
            speed_control_graph_canvas.chosen_speed_profile)
        self._devices_view_model.subscribe(speed_control_graph_canvas)
        speed_control_graph_canvas.subscribe(self._devices_view_model)

        for temp_source in temp_sources_and_profiles.keys():
            speed_control.temp_combo_box.addItem(temp_source.name)
        speed_control.temp_combo_box.currentTextChanged.connect(self.chosen_temp_source)
        for profiles in temp_sources_and_profiles.values():
            for profile in profiles:
                speed_control.profile_combo_box.addItem(profile)
            break  # add profiles for only the first temp_source
        speed_control.profile_combo_box.currentTextChanged.connect(self.chosen_speed_profile)
        return temp_sources_and_profiles

    def _device_temp_sources_and_profiles(
            self, channel_btn_id: str
    ) -> Tuple[Dict[TempSource, List[SpeedProfile]], Device]:
        """Iterates through all devices finding 'matches' to be used as temp sources and supported profiles"""
        temp_sources_and_profiles: Dict[TempSource, List[SpeedProfile]] = {}
        associated_device: Optional[Device] = None
        device_id, channel_name = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        for device in self._devices_view_model.devices:
            if device.lc_device_id != device_id and device.info.temp_ext_available and device.status.temps:
                for temp in device.status.temps:
                    if not self._temp_source_name_already_exists(temp_sources_and_profiles, temp.name):
                        available_profiles = self._get_available_profiles_for_ext_temp_sources()
                        temp_source = TempSource(temp.name, device)
                        temp_sources_and_profiles[temp_source] = available_profiles
            elif device.lc_device_id == device_id:
                associated_device = device
                for temp in device.status.temps:
                    lc_available_profiles = self._get_available_profiles_from(device, channel_name)
                    temp_source = TempSource(temp.name, device)
                    if lc_available_profiles:
                        self._remove_temp_source_if_name_exists(temp_sources_and_profiles, temp.name)
                        temp_sources_and_profiles[temp_source] = lc_available_profiles
        if associated_device is None:
            _LOG.error('No associated device found for channel button: %s !', channel_btn_id)
            raise ValueError('No associated device found for channel button')
        temp_sources_and_profiles = dict(sorted(temp_sources_and_profiles.items(), reverse=True))
        _LOG.debug('Initialized %s channel controller with options: %s', channel_btn_id, temp_sources_and_profiles)
        return temp_sources_and_profiles, associated_device

    @staticmethod
    def _temp_source_name_already_exists(
            temp_sources_and_profiles: Dict[TempSource, List[SpeedProfile]],
            temp_name: str) -> bool:
        return any(
            temp_source.name == temp_name
            for temp_source in temp_sources_and_profiles
        )

    @staticmethod
    def _remove_temp_source_if_name_exists(
            temp_sources_and_profiles: Dict[TempSource, List[SpeedProfile]],
            temp_name: str) -> None:
        matched_temp_sources = [
            temp_source
            for temp_source in temp_sources_and_profiles
            if temp_source.name == temp_name
        ]
        for temp_source in matched_temp_sources:
            temp_sources_and_profiles.pop(temp_source)

    @staticmethod
    def _get_available_profiles_from(device: Device, channel_name: str) -> List[SpeedProfile]:
        available_profiles: List[SpeedProfile] = [SpeedProfile.NONE]
        try:
            channel_info = device.info.channels[channel_name]
            if channel_info.speed_options.fixed_enabled:
                available_profiles.append(SpeedProfile.FIXED)
            if channel_info.speed_options.profiles_enabled or channel_info.speed_options.manual_profiles_enabled:
                available_profiles.append(SpeedProfile.CUSTOM)
        except AttributeError:
            _LOG.warning('Speed profiles inaccessible for %s in channel: %s', device.name_short, channel_name)
            return []
        return available_profiles

    @staticmethod
    def _get_available_profiles_for_ext_temp_sources() -> List[SpeedProfile]:
        return [SpeedProfile.NONE, SpeedProfile.FIXED, SpeedProfile.CUSTOM]

    def chosen_temp_source(self, temp_source_name: str) -> None:
        temp_source_btn = self.sender()
        channel_btn_id = temp_source_btn.objectName()
        _LOG.debug('Temp source chosen:  %s from %s', temp_source_name, channel_btn_id)
        device_control = self._channel_button_device_controls[channel_btn_id]
        speed_profiles = next(
            (p for ts, p in device_control.temp_sources_and_profiles.items() if ts.name == temp_source_name),
            [SpeedProfile.NONE]
        )
        profile_combo_box = device_control.control_ui.profile_combo_box
        profile_combo_box.clear()
        profile_combo_box.addItems(speed_profiles)

    def chosen_speed_profile(self, profile: str) -> None:
        if profile:  # on profile list update .clear() sends an empty string
            profile_btn = self.sender()
            profile_btn_id = profile_btn.objectName()
            _LOG.debug('Speed profile chosen:   %s from %s', profile, profile_btn_id)
