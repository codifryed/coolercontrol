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
# Form generated from reading UI file 'main_pagesYHjZGY.ui'
#
# Created by: Qt User Interface Compiler version 5.15.2
#
# WARNING! All changes made in this file will be lost when recompiling UI file!
################################################################################

# type: ignore

from PySide6.QtCore import QRect, QCoreApplication, QMetaObject
from PySide6.QtGui import QFont, Qt
from PySide6.QtWidgets import QVBoxLayout, QStackedWidget, QWidget, QFrame, QSizePolicy, QScrollArea, QLabel, \
    QHBoxLayout


class Ui_MainPages(object):
    def setupUi(self, MainPages):
        if not MainPages.objectName():
            MainPages.setObjectName(u"MainPages")
        MainPages.resize(1833, 1173)
        self.main_pages_layout = QVBoxLayout(MainPages)
        self.main_pages_layout.setSpacing(0)
        self.main_pages_layout.setObjectName(u"main_pages_layout")
        self.main_pages_layout.setContentsMargins(5, 5, 5, 5)
        self.pages = QStackedWidget(MainPages)
        self.pages.setObjectName(u"pages")
        self.system_overview = QWidget()
        self.system_overview.setObjectName(u"system_overview")
        self.system_overview.setStyleSheet(u"font-size: 14pt")
        self.system_overview_layout = QVBoxLayout(self.system_overview)
        self.system_overview_layout.setSpacing(5)
        self.system_overview_layout.setObjectName(u"system_overview_layout")
        self.system_overview_layout.setContentsMargins(5, 5, 5, 5)
        self.system = QFrame(self.system_overview)
        self.system.setObjectName(u"system")
        sizePolicy = QSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.system.sizePolicy().hasHeightForWidth())
        self.system.setSizePolicy(sizePolicy)
        self.system.setStyleSheet(u"background: transparent;")
        self.system.setFrameShape(QFrame.NoFrame)
        self.system.setFrameShadow(QFrame.Raised)
        self.system_layout = QVBoxLayout(self.system)
        self.system_layout.setSpacing(10)
        self.system_layout.setObjectName(u"system_layout")
        self.system_layout.setContentsMargins(0, 0, 5, 0)

        self.system_overview_layout.addWidget(self.system)

        self.pages.addWidget(self.system_overview)
        self.liquidctl_device_page = QWidget()
        self.liquidctl_device_page.setObjectName(u"liquidctl_device_page")
        font = QFont()
        font.setPointSize(14)
        self.liquidctl_device_page.setFont(font)
        self.liquidctl_device_page.setStyleSheet(u"font-size: 14pt")
        self.liquidctl_device_layout = QVBoxLayout(self.liquidctl_device_page)
        self.liquidctl_device_layout.setSpacing(5)
        self.liquidctl_device_layout.setObjectName(u"liquidctl_device_layout")
        self.liquidctl_device_layout.setContentsMargins(5, 5, 5, 5)
        self.device_layout = QVBoxLayout()
        self.device_layout.setObjectName(u"device_layout")
        self.device_name = QLabel(self.liquidctl_device_page)
        self.device_name.setObjectName(u"device_name")
        sizePolicy1 = QSizePolicy(QSizePolicy.Preferred, QSizePolicy.Fixed)
        sizePolicy1.setHorizontalStretch(0)
        sizePolicy1.setVerticalStretch(0)
        sizePolicy1.setHeightForWidth(self.device_name.sizePolicy().hasHeightForWidth())
        self.device_name.setSizePolicy(sizePolicy1)
        self.device_name.setAlignment(Qt.AlignHCenter | Qt.AlignTop)

        self.device_layout.addWidget(self.device_name)

        self.scrollArea = QScrollArea(self.liquidctl_device_page)
        self.scrollArea.setObjectName(u"scrollArea")
        self.scrollArea.setStyleSheet(u"background: transparent;")
        self.scrollArea.setWidgetResizable(True)
        self.device_contents = QWidget()
        self.device_contents.setObjectName(u"device_contents")
        self.device_contents.setGeometry(QRect(0, 0, 1809, 28))
        sizePolicy1.setHeightForWidth(self.device_contents.sizePolicy().hasHeightForWidth())
        self.device_contents.setSizePolicy(sizePolicy1)
        self.device_contents_layout = QVBoxLayout(self.device_contents)
        self.device_contents_layout.setObjectName(u"device_contents_layout")
        self.device_contents_layout.setContentsMargins(5, 5, 5, 5)
        self.speed_control_layout = QHBoxLayout()
        self.speed_control_layout.setObjectName(u"speed_control_layout")

        self.device_contents_layout.addLayout(self.speed_control_layout)

        self.lighting_control_layout = QHBoxLayout()
        self.lighting_control_layout.setObjectName(u"lighting_control_layout")

        self.device_contents_layout.addLayout(self.lighting_control_layout)

        self.other_control_layout = QHBoxLayout()
        self.other_control_layout.setObjectName(u"other_control_layout")

        self.device_contents_layout.addLayout(self.other_control_layout)

        self.scrollArea.setWidget(self.device_contents)

        self.device_layout.addWidget(self.scrollArea)

        self.liquidctl_device_layout.addLayout(self.device_layout)

        self.pages.addWidget(self.liquidctl_device_page)
        self.device_page_2 = QWidget()
        self.device_page_2.setObjectName(u"device_page_2")
        self.device_page_2.setLayoutDirection(Qt.LeftToRight)
        self.device_page_2_layout = QVBoxLayout(self.device_page_2)
        self.device_page_2_layout.setSpacing(5)
        self.device_page_2_layout.setObjectName(u"device_page_2_layout")
        self.device_page_2_layout.setContentsMargins(5, 5, 5, 5)
        self.pages.addWidget(self.device_page_2)

        self.main_pages_layout.addWidget(self.pages)

        self.retranslateUi(MainPages)

        self.pages.setCurrentIndex(1)

        QMetaObject.connectSlotsByName(MainPages)

    # setupUi

    def retranslateUi(self, MainPages):
        MainPages.setWindowTitle(QCoreApplication.translate("MainPages", u"Form", None))
        self.device_name.setText(QCoreApplication.translate("MainPages", u"Device Name", None))
    # retranslateUi
