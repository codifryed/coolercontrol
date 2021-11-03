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

from typing import List

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QLabel

from models.device import Device, DeviceType


class InfoPage(QLabel):

    def __init__(self, devices: List[Device]) -> None:
        super().__init__()
        detected_devices = '<h3>Detected Devices:</h3>'
        cpu_text = ''
        gpu_text = ''
        lc_text = ''
        debug_text = '<p>To enable debug output run with the \'--deug\' option.</p>'
        git_text = '<p>Issues and contributions at the <a href="https://gitlab.com/codifryed/coolero">' \
                   'GitLab Repo</a>.</p>'
        for device in devices:
            if device.device_type == DeviceType.CPU:
                cpu_text += f'<h4>CPU</h4>{device.device_name}<br>'
            if device.device_type == DeviceType.GPU:
                gpu_text += f'<h4>GPU</h4>{device.device_name}<br>'
            if device.device_type == DeviceType.LIQUIDCTL:
                lc_text += f'<h4>Liquidctl device #{device.lc_device_id + 1}</h4>{device.device_name}<br>'
        self.setTextFormat(Qt.TextFormat.RichText)
        self.setStyleSheet('font: 14px')
        self.setText(detected_devices + cpu_text + gpu_text + lc_text + debug_text + git_text)
