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
from typing import List, Dict, Optional, TYPE_CHECKING

from PySide6 import QtCore
from PySide6.QtCore import Qt, QObject
from PySide6.QtWidgets import QHBoxLayout, QBoxLayout, QToolButton, QWidget

from models.channel_info import ChannelInfo
from models.device_layouts import DeviceLayouts
from models.device_status import DeviceStatus
from services.dynamic_controls import DynamicControls
from view.uis.windows.main_window import MainFunctions
from view.uis.windows.main_window.scroll_area_style import SCROLL_AREA_STYLE
from view.widgets import PyLeftMenu
from view.widgets.channel_button.channel_button import ChannelButton
from view.widgets.channel_group_box.channel_group_box import ChannelGroupBox
from view_models.devices_view_model import DevicesViewModel

if TYPE_CHECKING:
    from coolero import MainWindow

_LOG = logging.getLogger(__name__)


class DynamicButtons(QObject):

    def __init__(self,
                 devices_view_model: DevicesViewModel,
                 main_window: MainWindow
                 ) -> None:
        super().__init__()
        self._device_statuses: List[DeviceStatus] = devices_view_model.device_statuses
        self._main_window = main_window
        self._left_menu: PyLeftMenu = main_window.ui.left_menu
        self._menu_btn_device_layouts: Dict[str, DeviceLayouts] = {}
        self._channel_button_device_controls: Dict[str, QWidget] = {}
        self._dynamic_controls = DynamicControls(devices_view_model, main_window)

    def create_menu_buttons_from_liquidctl_devices(self) -> None:
        """dynamically adds a device button to the left menu for each initialized liquidctl device"""
        for device_status in self._device_statuses:
            if device_status.lc_device_id is not None:
                btn_id = f"btn_liquidctl_{device_status.lc_device_id}"
                self._left_menu.add_menu_button(
                    btn_icon='icon_widgets.svg',
                    btn_id=btn_id,
                    btn_text=device_status.device_name_short,
                    btn_tooltip=device_status.device_name_short,
                    show_top=True,
                    is_active=False
                )
                _LOG.debug('added %s button to menu with id: %s', device_status.device_name_short, btn_id)
                self._create_layouts_for_device(btn_id, device_status)

    def set_liquidctl_device_page(self, btn_id: str) -> None:
        device_layouts: DeviceLayouts = self._menu_btn_device_layouts[btn_id]
        device_status = device_layouts.device_status
        self._left_menu.select_only_one(btn_id)
        self._main_window.clear_left_sub_menu()
        MainFunctions.set_page(self._main_window, self._main_window.ui.load_pages.liquidctl_device_page)
        self._set_device_page_stylesheet()
        self._set_device_page_title(device_status)
        self._remove_all_items_from_layout(self._main_window.ui.load_pages.device_contents_layout)
        if device_layouts.speed_layout is not None:
            self._main_window.ui.load_pages.device_contents_layout.addWidget(device_layouts.speed_layout)
        if device_layouts.lighting_layout is not None:
            self._main_window.ui.load_pages.device_contents_layout.addWidget(device_layouts.lighting_layout)
        if device_layouts.other_layout is not None:
            self._main_window.ui.load_pages.device_contents_layout.addWidget(device_layouts.other_layout)

    def _create_layouts_for_device(self, btn_id: str, device_status: DeviceStatus) -> None:
        speed_channels = {}
        lighting_channels = {}
        for channel, channel_info in device_status.device_info.channels.items():
            if channel_info.speed_options:
                speed_channels[channel] = channel_info
            elif channel_info.lighting_modes:
                lighting_channels[channel] = channel_info
        device_speed_layout = self._create_speed_control_layout(btn_id, speed_channels)
        device_lighting_layout = self._create_lighting_control_layout(btn_id, lighting_channels)
        device_other_layout = self._create_other_control_layout(btn_id, device_status)

        self._menu_btn_device_layouts[btn_id] = DeviceLayouts(
            device_status=device_status,
            speed_layout=device_speed_layout,
            lighting_layout=device_lighting_layout,
            other_layout=device_other_layout
        )
        _LOG.debug('added %s control layouts associated with button id: %s', device_status.device_name_short, btn_id)

    def _create_speed_control_layout(self,
                                     btn_id: str,
                                     speed_channels: Dict[str, ChannelInfo]
                                     ) -> Optional[ChannelGroupBox]:
        if not speed_channels:
            return None
        speed_box = ChannelGroupBox(
            title='Speed Channels',
            color=self._main_window.themes["app_color"]["text_foreground"],
            bg_color=self._main_window.themes["app_color"]["bg_one"],
            boarder_color=self._main_window.themes["app_color"]["text_foreground"],
        )
        speed_layout = QHBoxLayout(speed_box)
        speed_layout.setObjectName("speed_control_layout")
        speed_layout.setAlignment(Qt.AlignLeft)
        for channel in speed_channels:
            channel_button_id = f'{btn_id}_{channel}'
            # todo: display channel info the button itself (rpm, color, etc)
            channel_button = ChannelButton(
                text=channel.capitalize(),
                object_name=channel_button_id,
                color=self._main_window.themes["app_color"]["text_foreground"],
                bg_color=self._main_window.themes["app_color"]["dark_one"],
                bg_color_hover=self._main_window.themes["app_color"]["dark_three"],
                active_color=self._main_window.themes["app_color"]["context_color"]
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
            color=self._main_window.themes["app_color"]["text_foreground"],
            bg_color=self._main_window.themes["app_color"]["bg_one"],
            boarder_color=self._main_window.themes["app_color"]["text_foreground"],
        )
        lighting_layout = QHBoxLayout(lighting_box)
        lighting_layout.setObjectName("lighting_control_layout")
        lighting_layout.setAlignment(Qt.AlignLeft)
        for channel in lighting_channels:
            channel_button_id = f'{btn_id}_{channel}'
            # todo: display channel info the button itself (rpm, color, etc)
            channel_button = ChannelButton(
                text=channel.capitalize(),
                object_name=channel_button_id,
                color=self._main_window.themes["app_color"]["text_foreground"],
                bg_color=self._main_window.themes["app_color"]["dark_one"],
                bg_color_hover=self._main_window.themes["app_color"]["dark_three"],
                active_color=self._main_window.themes["app_color"]["context_color"]
            )
            channel_button.clicked.connect(self.channel_button_toggled)  # pylint: disable=no-member
            lighting_layout.addWidget(channel_button)
            self._channel_button_device_controls[channel_button_id] = \
                self._dynamic_controls.create_lighting_control(channel, channel_button_id)
        return lighting_box

    def _create_other_control_layout(self, btn_id: str, device_status: DeviceStatus) -> Optional[ChannelGroupBox]:
        # todo: for future devices with special control layouts:
        return None

    def _set_device_page_stylesheet(self) -> None:
        self._main_window.ui.load_pages.device_contents.setStyleSheet(
            f'background: {self._main_window.themes["app_color"]["bg_one"]};')
        self._main_window.ui.load_pages.scrollArea.setStyleSheet(
            SCROLL_AREA_STYLE.format(
                _scroll_bar_bg_color=self._main_window.themes["app_color"]["bg_one"],
                _scroll_bar_btn_color=self._main_window.themes["app_color"]["dark_four"],
                _context_color=self._main_window.themes["app_color"]["context_color"],
                _bg_color=self._main_window.themes["app_color"]["bg_one"]
            )
        )

    def _set_device_page_title(self, device_status: DeviceStatus) -> None:
        firmware_version = device_status.status.firmware_version \
            if device_status.status.firmware_version else device_status.lc_init_firmware_version
        device_name = f'<h3>{device_status.device_name}</h3>'
        device_label = f'{device_name}<small><i>firmware: v{firmware_version}</i></small>' \
            if firmware_version else device_name
        self._main_window.ui.load_pages.device_name.setTextFormat(Qt.TextFormat.RichText)
        self._main_window.ui.load_pages.device_name.setText(device_label)

    @staticmethod
    def _remove_all_items_from_layout(layout: QBoxLayout) -> None:
        while layout.takeAt(0) is not None:
            pass

    @QtCore.Slot()  # type: ignore[operator]
    def channel_button_toggled(self, checked: bool) -> None:
        channel_btn = self.sender()
        channel_btn_id = channel_btn.objectName()
        _LOG.debug('Channel Button: %s was toggled', channel_btn_id)
        self._only_one_channel_button_should_be_checked(channel_btn_id)
        if checked:
            self._show_corresponding_device_column_control_widget(channel_btn_id)
            if not MainFunctions.device_column_is_visible(self._main_window):
                MainFunctions.toggle_device_column(self._main_window)
        elif not checked and MainFunctions.device_column_is_visible(self._main_window):
            MainFunctions.toggle_device_column(self._main_window)

    def _only_one_channel_button_should_be_checked(self, channel_btn_id: str) -> None:
        for btn in self._main_window.ui.load_pages.device_contents.findChildren(QToolButton):
            if btn.objectName() != channel_btn_id:
                btn.setChecked(False)

    def _show_corresponding_device_column_control_widget(self, channel_btn_id: str) -> None:
        for btn_id, widget in self._channel_button_device_controls.items():
            if btn_id == channel_btn_id:
                if widget.parent() is None:
                    self._main_window.ui.device_column.device_layout.addWidget(widget)
                widget.show()
            else:
                widget.hide()
