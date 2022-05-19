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
from coolero.view.uis.windows.main_window.scroll_area_style import SCROLL_AREA_STYLE


class InfoPage(QScrollArea):

    def __init__(self, devices: List[Device]) -> None:
        super().__init__()
        self.setStyleSheet(
            SCROLL_AREA_STYLE.format(
                _scroll_bar_bg_color=Settings.theme["app_color"]["bg_one"],
                _scroll_bar_btn_color=Settings.theme["app_color"]["dark_four"],
                _context_color=Settings.theme["app_color"]["context_color"],
                _bg_color=Settings.theme["app_color"]["bg_one"]
            ) + f';font: 13pt; background: {Settings.theme["app_color"]["bg_two"]};'
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
        self._usage_info()
        self._base_layout.addItem(self._spacer())

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
            f'<center><h2>Coolero v{Settings.app["version"]}</h2></center>'
            f'<p><a href="{Settings.app["urls"]["repo"]}/-/blob/main/README.md" '
            f'style="color: {Settings.theme["app_color"]["context_color"]};">Documentation</a></p>'
            f'<p><a href="{Settings.app["urls"]["repo"]}/-/blob/main/CHANGELOG.md" '
            f'style="color: {Settings.theme["app_color"]["context_color"]}">Changelog</a></p>'
            f'<p><a href="{Settings.app["urls"]["repo"]}" '
            f'style="color: {Settings.theme["app_color"]["context_color"]}">Git Repository</a></p>'
            f'<p><a href="{Settings.app["urls"]["repo"]}/-/blob/main/CONTRIBUTING.md" '
            f'style="color: {Settings.theme["app_color"]["context_color"]}">Issues and Requests</a></p>'
            f'This program comes with absolutely no warranty.'
        )
        self._base_layout.addWidget(label)

    def _detected_devices(self, devices: List[Device]) -> None:
        label = QLabel()
        self._apply_default_label_properties(label)
        detected_devices = '<center><h3>Detected Devices:</h3></center>'
        cpu_text = ''
        gpu_text = ''
        lc_text = ''
        hwmon_text = ''
        if not devices:
            lc_text = '<h4>None</h4>'
        else:
            for device in devices:
                if device.type == DeviceType.CPU:
                    cpu_text += f'<h4>CPU</h4>{device.name}<br>'
                if device.type == DeviceType.GPU:
                    gpu_text += f'<h4>GPU #{device.type_id}</h4>{device.name}<br>'
                if device.type == DeviceType.LIQUIDCTL:
                    lc_text += f'<h4>Liquidctl device #{device.type_id}</h4>{device.name}<br>'
                if device.type == DeviceType.HWMON:
                    hwmon_text += f'<h4>Hwmon device #{device.type_id}</h4>{device.name}<br>'
                    if device.info.model is not None:
                        hwmon_text += f'{device.info.model}<br>'
        label.setText(detected_devices + cpu_text + gpu_text + lc_text + hwmon_text)
        self._base_layout.addWidget(label)

    def _usage_info(self) -> None:
        label = QLabel()
        self._apply_default_label_properties(label)
        label.setText(
            '<center><h3>Usage Tips:</h3></center>'
            '<b>Scroll or Right Click</b> - in the the system overview to zoom<br/><br/>'
            '<b>Left Click</b> - in any of the control panels to reapply settings<br/><br/>'
            '<b>Riglt Click</b> - in custom profile graph to add, remove, and reset points<br/><br/>'
            '<b>CTRL-Q</b> - to quit the application<br/><br/>'
            '<b>CTRL-H</b> - to hide the application window. Use the system tray menu to show again<br/><br/>'
            '<b>F5 or CTRL-R</b> - after applying a custom profile, use this to reset the profile back to the default'
        )
        self._base_layout.addWidget(label)
