#  Coolero - monitor and control your cooling and other devices
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
## Form generated from reading UI file 'lighting_controlFKKXGB.ui'
##
## Created by: Qt User Interface Compiler version 5.15.2
##
## WARNING! All changes made in this file will be lost when recompiling UI file!
################################################################################

from PySide6.QtCore import QCoreApplication, Qt, QMetaObject
from PySide6.QtWidgets import QSizePolicy, QVBoxLayout, QGroupBox, QWidget, QHBoxLayout, QLabel, QComboBox, QFrame


# type: ignore

class Ui_LightingControl(object):
    def setupUi(self, LightingControl):
        if not LightingControl.objectName():
            LightingControl.setObjectName(u"LightingControl")
        LightingControl.resize(759, 612)
        sizePolicy = QSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(LightingControl.sizePolicy().hasHeightForWidth())
        LightingControl.setSizePolicy(sizePolicy)
        LightingControl.setStyleSheet(u"QGroupBox {\n"
                                      "	font-size: 14pt;\n"
                                      "    border: 1px solid silver;\n"
                                      "    border-radius: 6px;\n"
                                      "    margin-top: 14px;\n"
                                      "}\n"
                                      "\n"
                                      "")
        self.form_layout = QVBoxLayout(LightingControl)
        self.form_layout.setObjectName(u"form_layout")
        self.form_layout.setContentsMargins(-1, 0, -1, -1)
        self.lighting_control_box = QGroupBox(LightingControl)
        self.lighting_control_box.setObjectName(u"lighting_control_box")
        self.lighting_control_box.setAlignment(Qt.AlignCenter)
        self.lighting_control_box.setChecked(False)
        self.box_layout = QVBoxLayout(self.lighting_control_box)
        self.box_layout.setObjectName(u"box_layout")
        self.content_widget = QWidget(self.lighting_control_box)
        self.content_widget.setObjectName(u"content_widget")
        self.content_layout = QVBoxLayout(self.content_widget)
        self.content_layout.setObjectName(u"content_layout")
        self.control_layout = QHBoxLayout()
        self.control_layout.setObjectName(u"control_layout")
        self.mode_layout = QVBoxLayout()
        self.mode_layout.setObjectName(u"mode_layout")
        self.mode_label = QLabel(self.content_widget)
        self.mode_label.setObjectName(u"mode_label")
        sizePolicy1 = QSizePolicy(QSizePolicy.Preferred, QSizePolicy.Fixed)
        sizePolicy1.setHorizontalStretch(0)
        sizePolicy1.setVerticalStretch(0)
        sizePolicy1.setHeightForWidth(self.mode_label.sizePolicy().hasHeightForWidth())
        self.mode_label.setSizePolicy(sizePolicy1)
        self.mode_label.setStyleSheet(u"margin-top:14px;")
        self.mode_label.setAlignment(Qt.AlignBottom | Qt.AlignHCenter)

        self.mode_layout.addWidget(self.mode_label)

        self.horizontalLayout = QHBoxLayout()
        self.horizontalLayout.setObjectName(u"horizontalLayout")
        self.mode_combo_box = QComboBox(self.content_widget)
        self.mode_combo_box.addItem("")
        self.mode_combo_box.setObjectName(u"mode_combo_box")
        sizePolicy2 = QSizePolicy(QSizePolicy.Maximum, QSizePolicy.Fixed)
        sizePolicy2.setHorizontalStretch(0)
        sizePolicy2.setVerticalStretch(0)
        sizePolicy2.setHeightForWidth(self.mode_combo_box.sizePolicy().hasHeightForWidth())
        self.mode_combo_box.setSizePolicy(sizePolicy2)
        self.mode_combo_box.setStyleSheet(u"margin-bottom: 14px;")
        self.mode_combo_box.setMinimumContentsLength(12)

        self.horizontalLayout.addWidget(self.mode_combo_box)

        self.mode_layout.addLayout(self.horizontalLayout)

        self.control_layout.addLayout(self.mode_layout)

        self.content_layout.addLayout(self.control_layout)

        self.controls_frame = QFrame(self.content_widget)
        self.controls_frame.setObjectName(u"controls_frame")
        sizePolicy.setHeightForWidth(self.controls_frame.sizePolicy().hasHeightForWidth())
        self.controls_frame.setSizePolicy(sizePolicy)
        self.controls_frame.setFrameShape(QFrame.NoFrame)
        self.controls_frame.setFrameShadow(QFrame.Raised)
        self.controls_layout = QVBoxLayout(self.controls_frame)
        self.controls_layout.setSpacing(5)
        self.controls_layout.setObjectName(u"controls_layout")
        self.controls_layout.setContentsMargins(5, 5, 5, 5)

        self.content_layout.addWidget(self.controls_frame)

        self.button_layout = QHBoxLayout()
        self.button_layout.setObjectName(u"button_layout")

        self.content_layout.addLayout(self.button_layout)

        self.box_layout.addWidget(self.content_widget)

        self.form_layout.addWidget(self.lighting_control_box)

        self.retranslateUi(LightingControl)

        QMetaObject.connectSlotsByName(LightingControl)

    # setupUi

    def retranslateUi(self, LightingControl):
        self.lighting_control_box.setTitle(QCoreApplication.translate("LightingControl", u"Channel", None))
        self.mode_label.setText(QCoreApplication.translate("LightingControl", u"LIGHTING MODE", None))
        self.mode_combo_box.setItemText(0, QCoreApplication.translate("LightingControl", u"None", None))

        self.mode_combo_box.setCurrentText(QCoreApplication.translate("LightingControl", u"None", None))
        pass
    # retranslateUi
