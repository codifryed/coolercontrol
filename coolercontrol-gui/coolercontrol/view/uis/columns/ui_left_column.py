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
# Form generated from reading UI file 'left_columnYHjZGY.ui'
#
# Created by: Qt User Interface Compiler version 5.15.2
#
# WARNING! All changes made in this file will be lost when recompiling UI file!
################################################################################

# type: ignore

from PySide6.QtCore import QMetaObject, QCoreApplication
from PySide6.QtWidgets import QVBoxLayout, QStackedWidget, QWidget


class Ui_LeftColumn(object):
    def setupUi(self, LeftColumn):
        if not LeftColumn.objectName():
            LeftColumn.setObjectName(u"LeftColumn")
        LeftColumn.resize(240, 600)
        self.main_pages_layout = QVBoxLayout(LeftColumn)
        self.main_pages_layout.setSpacing(0)
        self.main_pages_layout.setObjectName(u"main_pages_layout")
        self.main_pages_layout.setContentsMargins(5, 5, 5, 5)
        self.menus = QStackedWidget(LeftColumn)
        self.menus.setObjectName(u"menus")
        self.settings_page = QWidget()
        self.settings_page.setObjectName(u"settings_page")
        self.settings_page_layout = QVBoxLayout(self.settings_page)
        self.settings_page_layout.setSpacing(5)
        self.settings_page_layout.setObjectName(u"settings_page_layout")
        self.settings_page_layout.setContentsMargins(5, 5, 5, 5)
        self.menus.addWidget(self.settings_page)
        self.info_page = QWidget()
        self.info_page.setObjectName(u"info_page")
        self.info_page_layout = QVBoxLayout(self.info_page)
        self.info_page_layout.setSpacing(5)
        self.info_page_layout.setObjectName(u"info_page_layout")
        self.info_page_layout.setContentsMargins(5, 5, 5, 5)
        self.menus.addWidget(self.info_page)

        self.main_pages_layout.addWidget(self.menus)

        self.retranslateUi(LeftColumn)

        self.menus.setCurrentIndex(0)

        QMetaObject.connectSlotsByName(LeftColumn)

    # setupUi

    def retranslateUi(self, LeftColumn):
        LeftColumn.setWindowTitle(QCoreApplication.translate("LeftColumn", u"Form", None))
    # retranslateUi
