#  Coolero - monitor and control your cooling and other devices
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

import logging
from typing import List, Tuple, Dict, Optional

from PySide6.QtCore import QObject, Slot
from PySide6.QtWidgets import QWidget

from coolero.models.device import Device, DeviceType
from coolero.models.init_status import InitStatus
from coolero.models.speed_device_control import SpeedDeviceControl
from coolero.models.speed_profile import SpeedProfile
from coolero.models.temp_source import TempSource
from coolero.services.utils import ButtonUtils
from coolero.settings import Settings, ProfileSetting, UserSettings
from coolero.view.uis.canvases.speed_control_canvas import SpeedControlCanvas
from coolero.view.uis.controls.speed_control_style import SPEED_CONTROL_STYLE
from coolero.view.uis.controls.ui_speed_control import Ui_SpeedControl
from coolero.view_models.devices_view_model import DevicesViewModel

_LOG = logging.getLogger(__name__)


class SpeedControls(QObject):

    def __init__(self, devices_view_model: DevicesViewModel) -> None:
        super().__init__()
        self._devices_view_model = devices_view_model
        self._channel_button_device_controls: Dict[str, SpeedDeviceControl] = {}

    def create_speed_control(self, channel_name: str, channel_button_id: str) -> QWidget:
        """Creates the speed control Widget for specific channel button"""
        device_control_widget, speed_control = self._setup_speed_control_ui(channel_button_id)
        temp_sources_and_profiles, speed_graph = self._initialize_speed_control_dynamic_properties(
            speed_control, channel_name, channel_button_id
        )
        self._channel_button_device_controls[channel_button_id] = SpeedDeviceControl(
            control_widget=device_control_widget,
            control_ui=speed_control,
            speed_graph=speed_graph,
            temp_sources_and_profiles=temp_sources_and_profiles
        )
        return device_control_widget

    def resume_speed_graph_animation(self, channel_button_id: str) -> None:
        if controls := self._channel_button_device_controls.get(channel_button_id):
            controls.speed_graph.resume()

    def pause_speed_graph_animation(self, channel_button_id: str) -> None:
        if controls := self._channel_button_device_controls.get(channel_button_id):
            controls.speed_graph.pause()

    def pause_all_speed_graph_animations(self) -> None:
        for controls in self._channel_button_device_controls.values():
            controls.speed_graph.pause()

    @staticmethod
    def _setup_speed_control_ui(channel_button_id: str) -> Tuple[QWidget, Ui_SpeedControl]:
        device_control_widget = QWidget()
        device_control_widget.setObjectName(f"device_control_{channel_button_id}")
        speed_control = Ui_SpeedControl()
        speed_control.setupUi(device_control_widget)
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
        speed_control.temp_combo_box.view().parentWidget().setStyleSheet(
            f'background-color: {Settings.theme["app_color"]["dark_one"]};margin-top: 0; margin-bottom: 0;')
        speed_control.profile_combo_box.view().parentWidget().setStyleSheet(
            f'background-color: {Settings.theme["app_color"]["dark_one"]};margin-top: 0; margin-bottom: 0;')
        speed_control.content_widget.setStyleSheet('font-size: 14pt;')
        return device_control_widget, speed_control

    def _initialize_speed_control_dynamic_properties(
            self,
            speed_control: Ui_SpeedControl,
            channel_name: str,
            channel_button_id: str
    ) -> Tuple[Dict[TempSource, List[SpeedProfile]], SpeedControlCanvas]:
        speed_control.speed_control_box.setTitle(channel_name.capitalize())
        speed_control.temp_combo_box.setObjectName(channel_button_id)
        speed_control.temp_combo_box.clear()
        speed_control.profile_combo_box.setObjectName(channel_button_id)
        speed_control.profile_combo_box.clear()
        temp_sources_and_profiles, device = self._device_temp_sources_and_profiles(channel_button_id)

        starting_temp_source = None
        starting_speed_profile = None
        last_applied_temp_source_profile = Settings.get_last_applied_profile_for_channel(
            device.name, device.type_id, channel_name)
        if last_applied_temp_source_profile is not None:
            temp_source_name, profile_setting = last_applied_temp_source_profile
            for temp_source in temp_sources_and_profiles.keys():
                if temp_source.name == temp_source_name:
                    starting_temp_source = temp_source
                    starting_speed_profile = profile_setting.speed_profile
        if starting_temp_source is None:
            starting_temp_source = next(iter(temp_sources_and_profiles.keys()))
        if starting_speed_profile is None:
            chosen_profile = Settings.get_temp_source_chosen_profile(
                device.name, device.type_id, channel_name, starting_temp_source.name)
            if chosen_profile is None:
                starting_speed_profile = next(iter(next(iter(temp_sources_and_profiles.values()))), SpeedProfile.NONE)
            else:
                starting_speed_profile = chosen_profile.speed_profile

        init_status = InitStatus(complete=False)
        speed_control_graph_canvas = SpeedControlCanvas(
            device=device,
            channel_name=channel_name,
            starting_temp_source=starting_temp_source,
            temp_sources=list(temp_sources_and_profiles.keys()),
            init_status=init_status,
            starting_speed_profile=starting_speed_profile
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
        speed_control.temp_combo_box.setCurrentText(starting_temp_source.name)
        speed_control.temp_combo_box.currentTextChanged.connect(self.chosen_temp_source)
        for profile in temp_sources_and_profiles[starting_temp_source]:
            speed_control.profile_combo_box.addItem(profile)
        speed_control.profile_combo_box.setCurrentText(starting_speed_profile)
        speed_control.profile_combo_box.currentTextChanged.connect(self.chosen_speed_profile)

        # apply last applied settings to device
        if last_applied_temp_source_profile is not None and \
                Settings.user.value(UserSettings.LOAD_APPLIED_AT_STARTUP, defaultValue=True, type=bool):
            temp_source_name, _ = last_applied_temp_source_profile
            if temp_source_name == starting_temp_source.name:
                speed_control_graph_canvas.notify_observers()
        speed_control_graph_canvas.pause()  # pause all animations by default
        init_status.complete = True
        return temp_sources_and_profiles, speed_control_graph_canvas

    def _device_temp_sources_and_profiles(
            self, channel_btn_id: str
    ) -> Tuple[Dict[TempSource, List[SpeedProfile]], Device]:
        """Iterates through all devices finding 'matches' to be used as temp sources and supported profiles"""
        temp_sources_and_profiles: Dict[TempSource, List[SpeedProfile]] = {}
        associated_device: Optional[Device] = None
        device_id, channel_name, device_type = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        # display temp sources in a specific order:
        # first find device associated with this button and its temp profiles
        for device in self._devices_view_model.devices:
            if device.type == device_type and device.type_id == device_id:
                associated_device = device
                if device.type in [DeviceType.LIQUIDCTL, DeviceType.HWMON]:
                    for temp in device.status.temps:
                        available_profiles = self._get_available_profiles_from(device, channel_name)
                        temp_source = TempSource(temp.frontend_name, device)
                        if available_profiles:
                            temp_sources_and_profiles[temp_source] = available_profiles
        if associated_device is None:
            _LOG.error('No associated device found for channel button: %s !', channel_btn_id)
            raise ValueError('No associated device found for channel button')

        # next Show other associated device type temps
        for device in self._devices_view_model.devices:
            if device.type == associated_device.type \
                    and device.type_id != device_id \
                    and device.info.temp_ext_available and device.status.temps:
                for temp in device.status.temps:
                    available_profiles = self._get_available_profiles_for_ext_temp_sources(associated_device.type)
                    temp_source = TempSource(temp.external_name, device)
                    temp_sources_and_profiles[temp_source] = available_profiles
        # finally show other external device temps
        for device in self._devices_view_model.devices:
            if device.type != associated_device.type and device.info.temp_ext_available and device.status.temps:
                # ^CPUs are first, then comes GPUs & Others in the list, set by repo init
                for temp in device.status.temps:
                    available_profiles = self._get_available_profiles_for_ext_temp_sources(associated_device.type)
                    temp_source = TempSource(temp.external_name, device)
                    temp_sources_and_profiles[temp_source] = available_profiles

        if not temp_sources_and_profiles:  # if there are no temp sources (fan only controllers w/o cpu, gpu)
            temp_source = TempSource('None', associated_device)
            temp_sources_and_profiles[temp_source] = [SpeedProfile.DEFAULT, SpeedProfile.FIXED] \
                if associated_device.type == DeviceType.HWMON else [SpeedProfile.NONE, SpeedProfile.FIXED]
        _LOG.debug('Initialized %s channel controller with options: %s', channel_btn_id, temp_sources_and_profiles)
        return temp_sources_and_profiles, associated_device

    @staticmethod
    def _get_available_profiles_from(device: Device, channel_name: str) -> List[SpeedProfile]:
        available_profiles: List[SpeedProfile] = [SpeedProfile.DEFAULT] \
            if device.type == DeviceType.HWMON else [SpeedProfile.NONE]
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
    def _get_available_profiles_for_ext_temp_sources(device_type: DeviceType) -> List[SpeedProfile]:
        base_profile = [SpeedProfile.DEFAULT] if device_type == DeviceType.HWMON else [SpeedProfile.NONE]
        return base_profile + [SpeedProfile.FIXED, SpeedProfile.CUSTOM]

    @Slot()
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
        device_id, channel_name, device_type = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
        chosen_profile: Optional[ProfileSetting] = None
        for device in self._devices_view_model.devices:
            if device.type == device_type and device.type_id == device_id:
                chosen_profile = Settings.get_temp_source_chosen_profile(
                    device.name, device.type_id, channel_name, temp_source_name
                )
                break
        # addItems causes connections to also be triggered.
        device_control.control_ui.profile_combo_box.currentTextChanged.disconnect(self.chosen_speed_profile)
        if chosen_profile and chosen_profile.speed_profile in [SpeedProfile.FIXED, SpeedProfile.CUSTOM]:
            # workaround: is triggered twice when setting currentText to something other than the first profile
            device_control.control_ui.profile_combo_box.currentTextChanged.disconnect(
                device_control.speed_graph.chosen_speed_profile)
        profile_combo_box.addItems(speed_profiles)
        device_control.control_ui.profile_combo_box.currentTextChanged.connect(self.chosen_speed_profile)
        if chosen_profile and chosen_profile.speed_profile in [SpeedProfile.FIXED, SpeedProfile.CUSTOM]:
            device_control.control_ui.profile_combo_box.currentTextChanged.connect(
                device_control.speed_graph.chosen_speed_profile)
        if chosen_profile is not None:
            profile_combo_box.setCurrentText(chosen_profile.speed_profile)
        else:
            if device_type == DeviceType.HWMON:
                profile_combo_box.setCurrentText(SpeedProfile.DEFAULT)
            else:
                profile_combo_box.setCurrentText(SpeedProfile.NONE)

    @Slot()
    def chosen_speed_profile(self, profile: str) -> None:
        if profile:  # on profile list update .clear() sends an empty string
            profile_btn = self.sender()
            channel_btn_id = profile_btn.objectName()
            _LOG.debug('Speed profile chosen:   %s from %s', profile, channel_btn_id)
            device_control = self._channel_button_device_controls[channel_btn_id]
            temp_combo_box = device_control.control_ui.temp_combo_box
            temp_source_name = temp_combo_box.currentText()
            device_id, channel_name, device_type = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
            for device in self._devices_view_model.devices:
                if device.type == device_type and device.type_id == device_id:
                    Settings.save_chosen_profile_for_temp_source(
                        device.name, device.type_id, channel_name, temp_source_name, SpeedProfile[profile.upper()]
                    )
