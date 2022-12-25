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
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QResizeEvent, QPainterPath, QRegion
from PySide6.QtWidgets import QMessageBox, QGraphicsDropShadowEffect, QWidget

from coolercontrol.dialogs.dialog_style import DIALOG_STYLE
from coolercontrol.services.shell_commander import ShellCommander
from coolercontrol.settings import Settings

if TYPE_CHECKING:
    from coolercontrol.coolercontrol import Initialize

_LOG = logging.getLogger(__name__)


class UDevRulesDialog(QMessageBox):

    def __init__(self, parent: Initialize) -> None:
        super().__init__()
        self.splash_window = parent
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
        self.setWindowTitle('Problem')
        self.setText(
            '''
            <h3><center>Device Communication Issue</center></h3>
            <p><b>Liquidctl</b> has detected a communication problem with your device.</p>
            <p>This is most likely due to a <b>permissions</b> issue when accessing USB devices <b>not</b> as root.</p>
            <p>To give your user access to the system's USB devices you can apply some udev rules.</p>
            '''
        )
        self.setInformativeText(
            '<br><b>Do you want to apply the udev rules now?</b><br>'
            '<u>Restarting</u> your computer is most likely required for the changes to take effect.<br>'
        )
        self.setStandardButtons(QMessageBox.Abort | QMessageBox.No | QMessageBox.Yes)
        self.setButtonText(QMessageBox.Abort, 'Quit now!')
        self.setButtonText(QMessageBox.No, 'Nope')
        self.setButtonText(QMessageBox.Yes, 'Do it')
        self.setDefaultButton(QMessageBox.Yes)
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

    def run(self) -> None:
        self.window_frame.show()
        answer: int = self.exec()
        self.window_frame.close()
        if answer == QMessageBox.Abort:
            _LOG.info("Shutting down...")
            self.splash_window.main.devices_view_model.shutdown()
            self.splash_window.close()
        elif answer == QMessageBox.Yes:
            is_successful: bool = ShellCommander.apply_udev_rules()
            if is_successful:
                QMessageBox().information(
                    self, 'Success', 'Applying udev rules was successful. You may need to restart to apply the changes'
                )
            else:
                QMessageBox().warning(
                    self, 'Unsuccessful', 'Applying udev rules was unsuccessful. See log output for more details'
                )
