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

import logging

from PySide6.QtWidgets import QWidget

from coolero.services.dynamic_controls.lighting_controls import LightingControls
from coolero.services.dynamic_controls.speed_controls import SpeedControls
from coolero.view_models.devices_view_model import DevicesViewModel

_LOG = logging.getLogger(__name__)


class DynamicControls:

    def __init__(self, devices_view_model: DevicesViewModel) -> None:
        self._speed_controls = SpeedControls(devices_view_model)
        self._lighting_controls = LightingControls(devices_view_model)

    def create_speed_control(self, channel_name: str, channel_button_id: str) -> QWidget:
        """Creates the speed control Widget for specific channel button"""
        return self._speed_controls.create_speed_control(channel_name, channel_button_id)

    def create_lighting_control(self, channel_name: str, channel_button_id: str) -> QWidget:
        return self._lighting_controls.create_lighting_control(channel_name, channel_button_id)
