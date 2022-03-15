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

from PySide6.QtCore import Qt, QMargins
from PySide6.QtWidgets import QLabel, QVBoxLayout, QScrollArea, QFrame, QSpacerItem

from coolero.models.device import Device, DeviceType
from coolero.settings import Settings
from view.uis.windows.main_window.scroll_area_style import SCROLL_AREA_STYLE


class InfoPage(QScrollArea):

    def __init__(self, devices: List[Device]) -> None:
        super().__init__()
        self.setStyleSheet(
            SCROLL_AREA_STYLE.format(
                _scroll_bar_bg_color=Settings.theme["app_color"]["bg_one"],
                _scroll_bar_btn_color=Settings.theme["app_color"]["dark_four"],
                _context_color=Settings.theme["app_color"]["context_color"],
                _bg_color=Settings.theme["app_color"]["bg_one"]
            ) + f';font: 12pt; background: {Settings.theme["app_color"]["bg_two"]};'
        )
        self._base_layout = QVBoxLayout()
        self._base_layout.setAlignment(Qt.AlignTop)
        inner_frame_widget = QFrame(self)
        inner_frame_widget.setLayout(self._base_layout)
        self.setWidgetResizable(True)
        self.setWidget(inner_frame_widget)

        self._version_notice()
        self._base_layout.addWidget(self._line())
        self._detected_devices(devices)
        self._base_layout.addWidget(self._line())
        self._debug_text()
        self._base_layout.addItem(self._spacer())
        self._repo_text()

    @staticmethod
    def _apply_default_label_properties(label: QLabel) -> None:
        label.setAlignment(Qt.AlignTop)  # type: ignore
        label.setWordWrap(True)
        label.setOpenExternalLinks(True)
        label.setTextFormat(Qt.TextFormat.RichText)

    @staticmethod
    def _line() -> QFrame:
        return QFrame(  # type: ignore[call-arg]
            frameShape=QFrame.HLine, frameShadow=QFrame.Plain, minimumHeight=30, contentsMargins=QMargins(40, 0, 40, 0)
        )

    @staticmethod
    def _spacer() -> QSpacerItem:
        return QSpacerItem(1, 10)

    def _version_notice(self) -> None:
        label = QLabel()
        self._apply_default_label_properties(label)
        label.setText(
            f'<center><h3>Coolero v{Settings.app["version"]}</h3></center>' +
            'This program comes with absolutely no warranty.'
        )
        self._base_layout.addWidget(label)

    def _detected_devices(self, devices: List[Device]) -> None:
        label = QLabel()
        self._apply_default_label_properties(label)
        detected_devices = '<center><h3>Detected Devices:</h3></center>'
        cpu_text = ''
        gpu_text = ''
        lc_text = ''
        if not devices:
            lc_text = '<h4>None</h4>'
        else:
            for device in devices:
                if device.type == DeviceType.CPU:
                    cpu_text += f'<h4>CPU</h4>{device.name}<br>'
                if device.type == DeviceType.GPU:
                    gpu_text += f'<h4>GPU</h4>{device.name}<br>'
                if device.type == DeviceType.LIQUIDCTL:
                    lc_text += f'<h4>Liquidctl device #{device.lc_device_id}</h4>{device.name}<br>'
        label.setText(detected_devices + cpu_text + gpu_text + lc_text)
        self._base_layout.addWidget(label)

    def _debug_text(self) -> None:
        label = QLabel()
        self._apply_default_label_properties(label)
        label.setText('To enable debug output,<br>run with the \'--debug\' option.')
        self._base_layout.addWidget(label)

    def _repo_text(self) -> None:
        label = QLabel()
        self._apply_default_label_properties(label)
        label.setText(
            f'''For info, issues and contributions see the <a href="https://gitlab.com/codifryed/coolero" 
                       style="color: {Settings.theme["app_color"]["context_color"]}">Repo</a>.'''
        )
        self._base_layout.addWidget(label)
