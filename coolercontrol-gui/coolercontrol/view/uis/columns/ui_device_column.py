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

################################################################################
# Form generated from reading UI file 'device_columnYHjZGY.ui'
#
# Created by: Qt User Interface Compiler version 5.15.2
#
# WARNING! All changes made in this file will be lost when recompiling UI file!
################################################################################

# type: ignore

from PySide6.QtCore import QMetaObject, QCoreApplication
from PySide6.QtWidgets import QVBoxLayout, QFrame


class Ui_DeviceColumn(object):
    def setupUi(self, DeviceColumn):
        if not DeviceColumn.objectName():
            DeviceColumn.setObjectName(u"DeviceColumn")
        DeviceColumn.resize(585, 583)
        self.main_pages_layout = QVBoxLayout(DeviceColumn)
        self.main_pages_layout.setSpacing(0)
        self.main_pages_layout.setObjectName(u"main_pages_layout")
        self.main_pages_layout.setContentsMargins(5, 5, 5, 5)
        self.device_frame = QFrame(DeviceColumn)
        self.device_frame.setObjectName(u"device_frame")
        self.device_frame.setFrameShape(QFrame.NoFrame)
        self.device_frame.setFrameShadow(QFrame.Raised)
        self.device_layout = QVBoxLayout(self.device_frame)
        self.device_layout.setObjectName(u"device_layout")
        self.device_layout.setContentsMargins(5, 5, 5, 5)

        self.main_pages_layout.addWidget(self.device_frame)

        self.retranslateUi(DeviceColumn)

        QMetaObject.connectSlotsByName(DeviceColumn)

    # setupUi

    def retranslateUi(self, DeviceColumn):
        DeviceColumn.setWindowTitle(QCoreApplication.translate("DeviceColumn", u"Form", None))
    # retranslateUi
