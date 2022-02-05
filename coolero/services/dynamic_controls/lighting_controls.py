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
from typing import Dict

from PySide6.QtWidgets import QWidget

from models.device_control import DeviceControl
from settings import Settings
from view_models.devices_view_model import DevicesViewModel

_LOG = logging.getLogger(__name__)


class LightingControls(QWidget):

    def __init__(self,
                 devices_view_model: DevicesViewModel,
                 channel_button_device_controls: Dict[str, DeviceControl]
                 ) -> None:
        super().__init__()
        self._devices_view_model = devices_view_model
        self._channel_button_device_controls = channel_button_device_controls

    def create_lighting_control(self, channel_name: str, channel_button_id: str) -> QWidget:
        device_control_widget = QWidget()
        device_control_widget.setObjectName(f"device_control_{channel_button_id}")
        device_control_widget.setStyleSheet(f'''
                QGroupBox {{
                    color: {Settings.theme["app_color"]["text_foreground"]};
                    font-size: 14pt;
                    border: 1px solid {Settings.theme["app_color"]["text_foreground"]};
                    border-radius: 6px;
                    margin-top: 14px;
                }}
                ''')
        # todo: ADD LIGHTING CHANNEL CONTROL UI !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
        return device_control_widget
