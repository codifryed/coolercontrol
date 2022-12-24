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

from PySide6.QtCore import QRect, QMetaObject, QCoreApplication
from PySide6.QtGui import QFont, Qt
from PySide6.QtWidgets import QWidget, QVBoxLayout, QFrame, QLabel, QProgressBar


class Ui_SplashScreen(object):

    def setupUi(self, SplashScreen):
        if not SplashScreen.objectName():
            SplashScreen.setObjectName(u"SplashScreen")
        SplashScreen.resize(680, 400)
        self.centralwidget = QWidget(SplashScreen)
        self.centralwidget.setObjectName(u"centralwidget")
        self.verticalLayout = QVBoxLayout(self.centralwidget)
        self.verticalLayout.setSpacing(0)
        self.verticalLayout.setObjectName(u"verticalLayout")
        self.verticalLayout.setContentsMargins(10, 10, 10, 10)
        self.dropShadowFrame = QFrame(self.centralwidget)
        self.dropShadowFrame.setObjectName(u"dropShadowFrame")
        self.dropShadowFrame.setStyleSheet(u"QFrame {\n"
                                           "                                background-color: rgb(56, 58, 89);\n"
                                           "                                color: rgb(220, 220, 220);\n"
                                           "                                border-radius: 10px;\n"
                                           "                                }\n"
                                           "                            ")
        self.dropShadowFrame.setFrameShape(QFrame.StyledPanel)
        self.dropShadowFrame.setFrameShadow(QFrame.Raised)
        self.label_title = QLabel(self.dropShadowFrame)
        self.label_title.setObjectName(u"label_title")
        self.label_title.setGeometry(QRect(0, 90, 661, 61))
        font = QFont()
        font.setFamily(u"Segoe UI")
        font.setPointSize(40)
        self.label_title.setFont(font)
        self.label_title.setStyleSheet(u"color: rgb(98, 114, 164);")
        self.label_title.setAlignment(Qt.AlignCenter)
        self.label_description = QLabel(self.dropShadowFrame)
        self.label_description.setObjectName(u"label_description")
        self.label_description.setGeometry(QRect(0, 150, 661, 31))
        font1 = QFont()
        font1.setFamily(u"Segoe UI")
        font1.setPointSize(14)
        self.label_description.setFont(font1)
        self.label_description.setStyleSheet(u"color: rgb(98, 114, 164);")
        self.label_description.setAlignment(Qt.AlignCenter)
        self.progressBar = QProgressBar(self.dropShadowFrame)
        self.progressBar.setObjectName(u"progressBar")
        self.progressBar.setGeometry(QRect(50, 280, 561, 23))
        self.progressBar.setStyleSheet(u"QProgressBar {\n"
                                       "\n"
                                       "                                    background-color: rgb(98, 114, 164);\n"
                                       "                                    color: rgb(200, 200, 200);\n"
                                       "                                    border-style: none;\n"
                                       "                                    border-radius: 10px;\n"
                                       "                                    text-align: center;\n"
                                       "                                    }\n"
                                       "                                    QProgressBar::chunk{\n"
                                       "                                    border-radius: 10px;\n"
                                       "                                    background-color: qlineargradient("
                                       "spread:pad, x1:0, y1:0.511364, x2:1, y2:0.523,\n"
                                       "                                    stop:0 rgba(254, 121, 199, 255), "
                                       "stop:1 rgba(170, 85, 255, 255));\n"
                                       "                                    }\n"
                                       "                                ")
        self.progressBar.setValue(24)
        self.label_loading = QLabel(self.dropShadowFrame)
        self.label_loading.setObjectName(u"label_loading")
        self.label_loading.setGeometry(QRect(0, 320, 661, 21))
        font2 = QFont()
        font2.setFamily(u"Segoe UI")
        font2.setPointSize(12)
        self.label_loading.setFont(font2)
        self.label_loading.setStyleSheet(u"color: rgb(98, 114, 164);")
        self.label_loading.setAlignment(Qt.AlignCenter)
        self.label_version = QLabel(self.dropShadowFrame)
        self.label_version.setObjectName(u"label_version")
        self.label_version.setGeometry(QRect(20, 350, 621, 21))
        font3 = QFont()
        font3.setFamily(u"Segoe UI")
        font3.setPointSize(10)
        self.label_version.setFont(font3)
        self.label_version.setStyleSheet(u"color: rgb(98, 114, 164);")
        self.label_version.setAlignment(Qt.AlignRight | Qt.AlignTrailing | Qt.AlignVCenter)

        self.verticalLayout.addWidget(self.dropShadowFrame)

        SplashScreen.setCentralWidget(self.centralwidget)

        self.retranslateUi(SplashScreen)

        QMetaObject.connectSlotsByName(SplashScreen)

    def retranslateUi(self, SplashScreen):
        SplashScreen.setWindowTitle(QCoreApplication.translate("SplashScreen", u"MainWindow", None))
        self.label_title.setText(QCoreApplication.translate("SplashScreen", u"<strong>Coolero</strong>", None))
        self.label_description.setText(
            QCoreApplication.translate("SplashScreen", u"<strong>control</strong> your cooling", None))
        self.label_loading.setText(QCoreApplication.translate("SplashScreen", u"loading...", None))
        self.label_version.setText(QCoreApplication.translate("SplashScreen", u"<strong>version</strong>: 1.0.0", None))
