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

import logging

from PySide6.QtCore import Qt
from PySide6.QtGui import QResizeEvent
from PySide6.QtWidgets import QMessageBox, QWidget

from coolercontrol.dialogs.dialog_style import DIALOG_STYLE
from coolercontrol.dialogs.dialog_window_style import DIALOG_WINDOW_STYLE
from coolercontrol.settings import Settings
from coolercontrol.view.widgets.py_frameless_window.linux import LinuxFramelessWindowBase

log = logging.getLogger(__name__)


class ThinkPadFanControlDialog(QMessageBox, LinuxFramelessWindowBase):

    def __init__(self, parent=None) -> None:
        super().__init__(parent=parent)
        self._initFrameless()
        self.setObjectName("dialog_window")
        self.titleBar.minBtn.hide()
        self.titleBar.maxBtn.hide()
        self.titleBar.closeBtn.hide()
        self.titleBar.setDoubleClickEnabled(False)
        self.setResizeEnabled(False)
        self.setContentsMargins(5, 5, 5, 5)
        self.window_frame = QWidget()
        self.window_frame.setAttribute(Qt.WA_TranslucentBackground)
        self.window_frame.setWindowFlag(Qt.WindowStaysOnTopHint)
        self.setParent(self.window_frame)
        self.setTextFormat(Qt.TextFormat.RichText)
        self.setWindowFlag(Qt.WindowStaysOnTopHint)
        self.setWindowTitle("ThinkPad Fan Control")
        self.setText(
            '''
            <h3><center>ThinkPad Fan Control</center></h3>
            <p>Fan control operations are disabled by default for safety reasons. CoolerControl can try to enable this for you, 
            but you should be aware of the risks to your hardware. Proceed at your own risk.</p>
            '''
        )
        self.setInformativeText(
            '''
            <p>Are you sure you want to continue?</p>
            '''
        )
        self.setStandardButtons(QMessageBox.Yes | QMessageBox.No)
        self.setDefaultButton(QMessageBox.No)
        self.setStyleSheet(DIALOG_STYLE.format(
            _text_size=Settings.app["font"]["text_size"],
            _font_family=Settings.app["font"]["family"],
            _text_color=Settings.theme["app_color"]["text_foreground"],
            _bg_color=Settings.theme["app_color"]["bg_three"]
        ))
        self.setStyleSheet(DIALOG_WINDOW_STYLE.format(
            _text_size=Settings.app["font"]["text_size"],
            _font_family=Settings.app["font"]["family"],
            _text_color=Settings.theme["app_color"]["text_foreground"],
            _bg_color=Settings.theme["app_color"]["bg_three"]
        ))

    def resizeEvent(self, event: QResizeEvent) -> None:
        self.titleBar.resize(self.width(), self.titleBar.height())
        self.move(0, 0)  # this fixes a placement issue on x11

    def eventFilter(self, obj, event):
        return LinuxFramelessWindowBase.eventFilter(self, obj, event)

    def ask(self) -> bool:
        self.window_frame.show()
        response: int = self.exec()
        self.window_frame.close()
        return response == QMessageBox.Yes
