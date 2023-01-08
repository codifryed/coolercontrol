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

from PySide6.QtCore import Qt
from PySide6.QtGui import QResizeEvent, QPainterPath, QRegion
from PySide6.QtWidgets import QMessageBox, QCheckBox, QWidget

from coolero.dialogs.dialog_style import DIALOG_STYLE
from coolero.settings import Settings, UserSettings

_LOG = logging.getLogger(__name__)


class HwmonDaemonDialog(QMessageBox):

    def __init__(self) -> None:
        super().__init__()
        self.window_frame = QWidget()
        self.window_frame.setWindowFlag(Qt.FramelessWindowHint)
        self.window_frame.setAttribute(Qt.WA_TranslucentBackground)
        self.window_frame.setWindowFlag(Qt.WindowStaysOnTopHint)
        self.setParent(self.window_frame)
        self._dialog_style = DIALOG_STYLE.format(
            _text_size=Settings.app["font"]["text_size"],
            _font_family=Settings.app["font"]["family"],
            _text_color=Settings.theme["app_color"]["text_foreground"],
            _bg_color=Settings.theme["app_color"]["bg_three"]
        )
        self.setTextFormat(Qt.TextFormat.RichText)
        self.setWindowFlag(Qt.FramelessWindowHint)
        self.setWindowFlag(Qt.WindowStaysOnTopHint)
        self.setWindowTitle('Hwmon Daemon')
        self.setText(
            '''
            <center><h4>Hwmon Write Access</h4></center>
            <p>To be able to write to Hwmon devices, a daemon running with privileged access needs to be started at 
            startup.</p>
            '''
        )
        self.setInformativeText(
            '''
            Do you want to enable this feature now?<br/>
            <i>*Requires Coolero restart</i>
            '''
        )

        self.setStandardButtons(QMessageBox.Yes | QMessageBox.No)
        self.setDefaultButton(QMessageBox.Yes)
        self.check_box = QCheckBox("Don't ask me again")
        self.setCheckBox(self.check_box)
        self.setStyleSheet(self._dialog_style)

    def resizeEvent(self, event: QResizeEvent) -> None:
        """
        Allows us to have rounded corners on the window.
        This has to be done after the window is drawn to have the correct size
        """
        radius = 10
        path = QPainterPath()
        path.addRoundedRect(self.rect(), radius, radius)
        self.setMask(QRegion(path.toFillPolygon().toPolygon()))
        self.window_frame.setFixedSize(self.size())
        self.move(0, 0)  # this fixes a placement issue on x11

    def ask(self) -> bool:
        self.window_frame.show()
        enable_hwmon_write_daemon_answer: int = self.exec()
        self.window_frame.close()
        enable_hwmon_write_daemon: bool = (enable_hwmon_write_daemon_answer == QMessageBox.Yes)
        if self.check_box.isChecked():
            Settings.user.setValue(UserSettings.SHOW_HWMON_DIALOG, False)
        return enable_hwmon_write_daemon
