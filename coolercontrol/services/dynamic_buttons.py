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
from typing import List, Dict, Optional, TYPE_CHECKING

from PySide6.QtCore import Qt, QObject, Slot
from PySide6.QtWidgets import QHBoxLayout, QBoxLayout, QToolButton, QWidget, QGroupBox

from coolero.models.channel_info import ChannelInfo
from coolero.models.device import Device, DeviceType
from coolero.models.device_layouts import DeviceLayouts
from coolero.services.dynamic_controls.dynamic_controls_main import DynamicControls
from coolero.services.utils import ButtonUtils
from coolero.view.uis.windows.main_window import MainFunctions
from coolero.view.uis.windows.main_window.scroll_area_style import SCROLL_AREA_STYLE
from coolero.view.widgets import PyLeftMenu
from coolero.view.widgets.channel_button.channel_button import ChannelButton
from coolero.view.widgets.channel_group_box.channel_group_box import ChannelGroupBox
from coolero.view_models.devices_view_model import DevicesViewModel

if TYPE_CHECKING:
    from coolero.app import MainWindow

_LOG = logging.getLogger(__name__)


class DynamicButtons(QObject):

    def __init__(self,
                 devices_view_model: DevicesViewModel,
                 main_window: MainWindow
                 ) -> None:
        super().__init__()
        self._devices: List[Device] = devices_view_model.devices
        self._main_window = main_window
        self._left_menu: PyLeftMenu = main_window.ui.left_menu
        self._menu_btn_device_layouts: Dict[str, DeviceLayouts] = {}
        self._channel_button_device_controls: Dict[str, QWidget] = {}
        self._dynamic_controls = DynamicControls(devices_view_model)
        devices_view_model.set_force_apply_fun(self._dynamic_controls.force_apply_settings)

    def create_menu_buttons_from_devices(self) -> None:
        """dynamically adds a device button to the left menu for each initialized device"""
        for device in self._devices:
            if device.type == DeviceType.LIQUIDCTL:
                btn_id = f"btn_liquidctl_{device.type_id}"
                self._left_menu.add_menu_button(
                    btn_icon='icon_widgets.svg',
                    btn_id=btn_id,
                    btn_text=device.name_short,
                    btn_tooltip=device.name_short,
                    show_top=True,
                    is_active=False
                )
                _LOG.debug('added %s button to menu with id: %s', device.name_short, btn_id)
                self._create_layouts_for_device(btn_id, device)
            elif device.type == DeviceType.HWMON and device.info.channels:  # some hwmon devices only have temps
                btn_id = f"btn_hwmon_{device.type_id}"
                self._left_menu.add_menu_button(
                    btn_icon='icon_widgets.svg',
                    btn_id=btn_id,
                    btn_text=device.name_short,
                    btn_tooltip=device.name_short,
                    show_top=True,
                    is_active=False
                )
                _LOG.debug('added %s button to menu with id: %s', device.name_short, btn_id)
                self._create_layouts_for_device(btn_id, device)

    def set_device_page(self, btn_id: str) -> None:
        device_layouts: DeviceLayouts = self._menu_btn_device_layouts[btn_id]
        device = device_layouts.device
        self._left_menu.select_only_one(btn_id)
        self._main_window.clear_left_sub_menu()
        MainFunctions.set_page(self._main_window, self._main_window.ui.load_pages.liquidctl_device_page)
        self._set_device_page_stylesheet()
        self._set_device_page_title(device)
        btn_device_id, _, btn_device_type = ButtonUtils.extract_info_from_channel_btn_id(btn_id)
        for device_layout in self._main_window.ui.load_pages.device_contents.findChildren(QGroupBox):
            layout_device_id, _, layout_device_type = ButtonUtils.extract_info_from_channel_btn_id(
                device_layout.objectName()
            )
            if layout_device_type == btn_device_type and layout_device_id == btn_device_id:
                device_layout.show()
            else:
                device_layout.hide()
        for channel_btn in self._main_window.ui.load_pages.device_contents.findChildren(QToolButton):
            channel_btn_id: str = channel_btn.objectName()
            channel_device_id, _, channel_device_type = ButtonUtils.extract_info_from_channel_btn_id(channel_btn_id)
            if channel_device_type == btn_device_type \
                    and channel_device_id == btn_device_id \
                    and channel_btn.isChecked():
                self._show_corresponding_device_column_control_widget(channel_btn_id)
                if not MainFunctions.device_column_is_visible(self._main_window):
                    MainFunctions.toggle_device_column(self._main_window)
                break
        else:
            self._dynamic_controls.pause_all_animations()
            if MainFunctions.device_column_is_visible(self._main_window):
                MainFunctions.toggle_device_column(self._main_window)

    def _create_layouts_for_device(self, btn_id: str, device: Device) -> None:
        speed_channels = {}
        lighting_channels = {}
        lcd_channels = {}
        for channel, channel_info in device.info.channels.items():
            if channel_info.speed_options:
                for ch in device.status.channels:
                    if channel == ch.name and ch.rpm is not None:  # make sure the channel is reporting
                        speed_channels[channel] = channel_info
                        break
                else:
                    if channel == 'sync':
                        # add 'sync' channel for devices that support it to control multiple speed devices
                        speed_channels[channel] = channel_info
            elif channel_info.lighting_modes:
                lighting_channels[channel] = channel_info
            elif channel_info.lcd_modes:
                lcd_channels[channel] = channel_info
        device_speed_layout = self._create_speed_control_layout(btn_id, speed_channels)
        device_lighting_layout = self._create_lighting_control_layout(btn_id, lighting_channels)
        device_other_layout = self._create_other_control_layout(btn_id, lcd_channels)

        self._menu_btn_device_layouts[btn_id] = DeviceLayouts(
            device=device,
            speed_layout=device_speed_layout,
            lighting_layout=device_lighting_layout,
            other_layout=device_other_layout
        )
        if device_speed_layout is not None:
            device_speed_layout.hide()
            self._main_window.ui.load_pages.device_contents_layout.addWidget(device_speed_layout)
        if device_lighting_layout is not None:
            device_lighting_layout.hide()
            self._main_window.ui.load_pages.device_contents_layout.addWidget(device_lighting_layout)
        if device_other_layout is not None:
            device_other_layout.hide()
            self._main_window.ui.load_pages.device_contents_layout.addWidget(device_other_layout)
        _LOG.debug('added %s control layouts associated with button id: %s', device.name_short, btn_id)

    def _create_speed_control_layout(self,
                                     btn_id: str,
                                     speed_channels: Dict[str, ChannelInfo]
                                     ) -> Optional[ChannelGroupBox]:
        if not speed_channels:
            return None
        speed_box = ChannelGroupBox(
            title='Speed Channels',
            color=self._main_window.theme["app_color"]["text_foreground"],
            bg_color=self._main_window.theme["app_color"]["bg_one"],
            boarder_color=self._main_window.theme["app_color"]["text_foreground"],
        )
        speed_box.setObjectName(f'{btn_id}_speed_button_group_box')
        speed_layout = QHBoxLayout(speed_box)
        speed_layout.setObjectName("speed_control_layout")
        speed_layout.setAlignment(Qt.AlignLeft)
        for channel in speed_channels:
            channel_button_id = f'{btn_id}_{channel}'
            channel_button = ChannelButton(
                text=channel.capitalize(),
                object_name=channel_button_id,
                color=self._main_window.theme["app_color"]["text_foreground"],
                bg_color=self._main_window.theme["app_color"]["dark_one"],
                bg_color_hover=self._main_window.theme["app_color"]["dark_three"],
                active_color=self._main_window.theme["app_color"]["context_color"]
            )
            channel_button.clicked.connect(self.channel_button_toggled)  # pylint: disable=no-member
            speed_layout.addWidget(channel_button)
            self._channel_button_device_controls[channel_button_id] = \
                self._dynamic_controls.create_speed_control(channel, channel_button_id)
        return speed_box

    def _create_lighting_control_layout(self,
                                        btn_id: str,
                                        lighting_channels: Dict[str, ChannelInfo]
                                        ) -> Optional[ChannelGroupBox]:
        if not lighting_channels:
            return None
        lighting_box = ChannelGroupBox(
            title='Lighting Channels',
            color=self._main_window.theme["app_color"]["text_foreground"],
            bg_color=self._main_window.theme["app_color"]["bg_one"],
            boarder_color=self._main_window.theme["app_color"]["text_foreground"],
        )
        lighting_box.setObjectName(f'{btn_id}_lighting_button_group_box')
        lighting_layout = QHBoxLayout(lighting_box)
        lighting_layout.setObjectName("lighting_control_layout")
        lighting_layout.setAlignment(Qt.AlignLeft)
        for channel in lighting_channels:
            channel_button_id = f'{btn_id}_{channel}'
            channel_button = ChannelButton(
                text=channel.capitalize(),
                object_name=channel_button_id,
                color=self._main_window.theme["app_color"]["text_foreground"],
                bg_color=self._main_window.theme["app_color"]["dark_one"],
                bg_color_hover=self._main_window.theme["app_color"]["dark_three"],
                active_color=self._main_window.theme["app_color"]["context_color"]
            )
            channel_button.clicked.connect(self.channel_button_toggled)  # pylint: disable=no-member
            lighting_layout.addWidget(channel_button)
            self._channel_button_device_controls[channel_button_id] = \
                self._dynamic_controls.create_lighting_control(channel, channel_button_id)
        return lighting_box

    def _create_other_control_layout(self, btn_id: str, lcd_channels: Dict[str, ChannelInfo]) -> ChannelGroupBox | None:
        """For Other/Special controls"""
        if not lcd_channels:
            return None
        other_box = ChannelGroupBox(
            title='Other Channels',
            color=self._main_window.theme["app_color"]["text_foreground"],
            bg_color=self._main_window.theme["app_color"]["bg_one"],
            boarder_color=self._main_window.theme["app_color"]["text_foreground"],
        )
        other_box.setObjectName(f'{btn_id}_other_button_group_box')
        other_layout = QHBoxLayout(other_box)
        other_layout.setObjectName("other_control_layout")
        other_layout.setAlignment(Qt.AlignLeft)
        lcd_button_id = f"{btn_id}_lcd"
        lcd_button = ChannelButton(
            text="LCD",
            object_name=lcd_button_id,
            color=self._main_window.theme["app_color"]["text_foreground"],
            bg_color=self._main_window.theme["app_color"]["dark_one"],
            bg_color_hover=self._main_window.theme["app_color"]["dark_three"],
            active_color=self._main_window.theme["app_color"]["context_color"]
        )
        lcd_button.clicked.connect(self.channel_button_toggled)  # pylint: disable=no-member
        other_layout.addWidget(lcd_button)
        self._channel_button_device_controls[lcd_button_id] = \
            self._dynamic_controls.create_lcd_control(lcd_button_id)
        return other_box

    def _set_device_page_stylesheet(self) -> None:
        self._main_window.ui.load_pages.device_contents.setStyleSheet(
            f'background: {self._main_window.theme["app_color"]["bg_one"]};')
        self._main_window.ui.load_pages.scrollArea.setStyleSheet(
            SCROLL_AREA_STYLE.format(
                _scroll_bar_bg_color=self._main_window.theme["app_color"]["bg_one"],
                _scroll_bar_btn_color=self._main_window.theme["app_color"]["dark_four"],
                _context_color=self._main_window.theme["app_color"]["context_color"],
                _bg_color=self._main_window.theme["app_color"]["bg_one"]
            )
        )

    def _set_device_page_title(self, device: Device) -> None:
        firmware_version = (
                device.status.firmware_version or device.lc_init_firmware_version
        )
        device_name = f'<h4 style="color: {self._main_window.theme["app_color"]["text_title"]}">' \
                      f'{device.name}</h4>'
        device_label = f'{device_name}<small><i>firmware: v{firmware_version}</i></small>' \
            if firmware_version else device_name
        self._main_window.ui.load_pages.device_name.setTextFormat(Qt.TextFormat.RichText)
        self._main_window.ui.load_pages.device_name.setText(device_label)

    @staticmethod
    def _remove_all_items_from_layout(layout: QBoxLayout) -> None:
        empty: bool = False
        while not empty:
            item = layout.takeAt(0)
            if item is None:
                empty = True
            elif item.widget() is not None:
                item.widget().setParent(None)  # type: ignore[call-overload]

    @Slot(bool)
    def channel_button_toggled(self, checked: bool) -> None:
        channel_btn = self.sender()
        channel_btn_id = channel_btn.objectName()
        _LOG.debug('Channel Button: %s was toggled', channel_btn_id)
        self.only_one_channel_button_should_be_checked_per_device(channel_btn_id)
        if checked:
            self._show_corresponding_device_column_control_widget(channel_btn_id)
            if not MainFunctions.device_column_is_visible(self._main_window):
                MainFunctions.toggle_device_column(self._main_window)
        else:
            self._dynamic_controls.pause_animation(channel_btn_id)
            if MainFunctions.device_column_is_visible(self._main_window):
                MainFunctions.toggle_device_column(self._main_window)

    def only_one_channel_button_should_be_checked_per_device(self, channel_btn_id: str) -> None:
        for btn in self._main_window.ui.load_pages.device_contents.findChildren(QToolButton):
            channel_btn_device_id, _, channel_btn_device_type = ButtonUtils.extract_info_from_channel_btn_id(
                channel_btn_id
            )
            btn_device_id, _, btn_device_type = ButtonUtils.extract_info_from_channel_btn_id(btn.objectName())
            if btn.objectName() != channel_btn_id \
                    and btn_device_id == channel_btn_device_id \
                    and btn_device_type == channel_btn_device_type:
                btn.setChecked(False)

    def uncheck_all_channel_buttons(self) -> None:
        for btn in self._main_window.ui.load_pages.device_contents.findChildren(QToolButton):
            btn.setChecked(False)
        self._dynamic_controls.pause_all_animations()

    def _show_corresponding_device_column_control_widget(self, channel_btn_id: str) -> None:
        for btn_id, widget in self._channel_button_device_controls.items():
            if btn_id == channel_btn_id:
                self._dynamic_controls.resume_animation(btn_id)
                if widget.parent() is None:
                    self._main_window.ui.device_column.device_layout.addWidget(widget)
                widget.show()
            else:
                self._dynamic_controls.pause_animation(btn_id)
                widget.hide()
