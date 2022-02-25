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

from coolero.models.device import Device, DeviceType
from coolero.settings import Settings


class InfoPage(QLabel):

    def __init__(self, devices: List[Device]) -> None:
        super().__init__()
        version = f'<h3>Coolero v{Settings.app["version"]}</h3>'
        notice = '<p>This program comes with absolutely no warranty.</p>'
        detected_devices = '<h3>Detected Devices:</h3>'
        cpu_text = ''
        gpu_text = ''
        lc_text = ''
        debug_text = '<p>To enable debug output run with the \'--debug\' option.</p>'
        git_text = f'''<p>For info, issues and contributions see the <a href="https://gitlab.com/codifryed/coolero" 
                       style="color: {Settings.theme["app_color"]["context_color"]}">Repo</a>.</p>'''

        word_wrap_padding = '<br><br>'
        for device in devices:
            if device.type == DeviceType.CPU:
                cpu_text += f'<h4>CPU</h4>{device.name}<br>'
            if device.type == DeviceType.GPU:
                gpu_text += f'<h4>GPU</h4>{device.name}<br>'
            if device.type == DeviceType.LIQUIDCTL:
                lc_text += f'<h4>Liquidctl device #{device.lc_device_id}</h4>{device.name}<br>'
        if not devices:
            lc_text = '<h4>No devices detected</h4>'
        self.setTextFormat(Qt.TextFormat.RichText)
        self.setStyleSheet('font: 12pt;')
        self.setWordWrap(True)
        self.setOpenExternalLinks(True)
        self.setText(
            version + notice + detected_devices + cpu_text + gpu_text + lc_text + debug_text + git_text
            + word_wrap_padding
        )
