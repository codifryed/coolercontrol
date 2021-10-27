#  Coolero - monitor and control your cooling and other devices
#  Copyright (c) 2021  Guy Boldon
#  All credit for basis of the user interface (GUI) goes to: Wanderson M.Pimenta
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

from PySide6.QtCore import Qt, QMetaObject, QCoreApplication
from PySide6.QtWidgets import QSizePolicy, QVBoxLayout, QGroupBox, QWidget, QHBoxLayout, QLabel, QComboBox, QFrame


class Ui_SpeedControl(object):
    def setupUi(self, SpeedControl):
        if not SpeedControl.objectName():
            SpeedControl.setObjectName(u"SpeedControl")
        SpeedControl.resize(522, 522)
        sizePolicy = QSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(SpeedControl.sizePolicy().hasHeightForWidth())
        SpeedControl.setSizePolicy(sizePolicy)
        SpeedControl.setStyleSheet(u"QGroupBox {\n"
                                   "	font-size: 14pt;\n"
                                   "    border: 1px solid silver;\n"
                                   "    border-radius: 6px;\n"
                                   "    margin-top: 14px;\n"
                                   "}\n"
                                   "\n"
                                   "")
        self.form_layout = QVBoxLayout(SpeedControl)
        self.form_layout.setObjectName(u"form_layout")
        self.form_layout.setContentsMargins(-1, 0, -1, -1)
        self.speed_control_box = QGroupBox(SpeedControl)
        self.speed_control_box.setObjectName(u"speed_control_box")
        self.speed_control_box.setAlignment(Qt.AlignCenter)
        self.speed_control_box.setChecked(False)
        self.box_layout = QVBoxLayout(self.speed_control_box)
        self.box_layout.setObjectName(u"box_layout")
        self.content_widget = QWidget(self.speed_control_box)
        self.content_widget.setObjectName(u"content_widget")
        self.content_layout = QVBoxLayout(self.content_widget)
        self.content_layout.setObjectName(u"content_layout")
        self.control_layout = QHBoxLayout()
        self.control_layout.setObjectName(u"control_layout")
        self.temp_layout = QVBoxLayout()
        self.temp_layout.setObjectName(u"temp_layout")
        self.temp_label = QLabel(self.content_widget)
        self.temp_label.setObjectName(u"temp_label")
        sizePolicy1 = QSizePolicy(QSizePolicy.Preferred, QSizePolicy.Fixed)
        sizePolicy1.setHorizontalStretch(0)
        sizePolicy1.setVerticalStretch(0)
        sizePolicy1.setHeightForWidth(self.temp_label.sizePolicy().hasHeightForWidth())
        self.temp_label.setSizePolicy(sizePolicy1)
        self.temp_label.setStyleSheet(u"margin-top:14px;")
        self.temp_label.setAlignment(Qt.AlignBottom | Qt.AlignHCenter)

        self.temp_layout.addWidget(self.temp_label)

        self.temp_combo_box = QComboBox(self.content_widget)
        self.temp_combo_box.addItem("")
        self.temp_combo_box.addItem("")
        self.temp_combo_box.addItem("")
        self.temp_combo_box.setObjectName(u"temp_combo_box")
        sizePolicy2 = QSizePolicy(QSizePolicy.Fixed, QSizePolicy.Fixed)
        sizePolicy2.setHorizontalStretch(0)
        sizePolicy2.setVerticalStretch(0)
        sizePolicy2.setHeightForWidth(self.temp_combo_box.sizePolicy().hasHeightForWidth())
        self.temp_combo_box.setSizePolicy(sizePolicy2)
        self.temp_combo_box.setStyleSheet(u"margin-bottom: 14px;")
        self.temp_combo_box.setMinimumContentsLength(12)

        self.temp_layout.addWidget(self.temp_combo_box)

        self.control_layout.addLayout(self.temp_layout)

        self.profile_layout = QVBoxLayout()
        self.profile_layout.setObjectName(u"profile_layout")
        self.profile_label = QLabel(self.content_widget)
        self.profile_label.setObjectName(u"profile_label")
        sizePolicy1.setHeightForWidth(self.profile_label.sizePolicy().hasHeightForWidth())
        self.profile_label.setSizePolicy(sizePolicy1)
        self.profile_label.setStyleSheet(u"margin-top: 14px;")
        self.profile_label.setAlignment(Qt.AlignBottom | Qt.AlignHCenter)

        self.profile_layout.addWidget(self.profile_label)

        self.profile_combo_box = QComboBox(self.content_widget)
        self.profile_combo_box.addItem("")
        self.profile_combo_box.addItem("")
        self.profile_combo_box.addItem("")
        self.profile_combo_box.setObjectName(u"profile_combo_box")
        sizePolicy2.setHeightForWidth(self.profile_combo_box.sizePolicy().hasHeightForWidth())
        self.profile_combo_box.setSizePolicy(sizePolicy2)
        self.profile_combo_box.setStyleSheet(u"margin-bottom:14px;")
        self.profile_combo_box.setMinimumContentsLength(12)

        self.profile_layout.addWidget(self.profile_combo_box)

        self.control_layout.addLayout(self.profile_layout)

        self.content_layout.addLayout(self.control_layout)

        self.graph_frame = QFrame(self.content_widget)
        self.graph_frame.setObjectName(u"graph_frame")
        sizePolicy.setHeightForWidth(self.graph_frame.sizePolicy().hasHeightForWidth())
        self.graph_frame.setSizePolicy(sizePolicy)
        self.graph_frame.setFrameShape(QFrame.NoFrame)
        self.graph_frame.setFrameShadow(QFrame.Raised)
        self.graph_layout = QVBoxLayout(self.graph_frame)
        self.graph_layout.setSpacing(5)
        self.graph_layout.setObjectName(u"graph_layout")
        self.graph_layout.setContentsMargins(5, 5, 5, 5)

        self.content_layout.addWidget(self.graph_frame)

        self.button_layout = QHBoxLayout()
        self.button_layout.setObjectName(u"button_layout")

        self.content_layout.addLayout(self.button_layout)

        self.box_layout.addWidget(self.content_widget)

        self.form_layout.addWidget(self.speed_control_box)

        self.retranslateUi(SpeedControl)

        QMetaObject.connectSlotsByName(SpeedControl)

    # setupUi

    def retranslateUi(self, SpeedControl):
        self.speed_control_box.setTitle(QCoreApplication.translate("SpeedControl", u"Fan/Pump", None))
        self.temp_label.setText(QCoreApplication.translate("SpeedControl", u"TEMP SOURCE", None))
        self.temp_combo_box.setItemText(0, QCoreApplication.translate("SpeedControl", u"Liquid", None))
        self.temp_combo_box.setItemText(1, QCoreApplication.translate("SpeedControl", u"CPU", None))
        self.temp_combo_box.setItemText(2, QCoreApplication.translate("SpeedControl", u"GPU", None))

        self.profile_label.setText(QCoreApplication.translate("SpeedControl", u"SPEED PROFILE", None))
        self.profile_combo_box.setItemText(0, QCoreApplication.translate("SpeedControl", u"None", None))
        self.profile_combo_box.setItemText(1, QCoreApplication.translate("SpeedControl", u"Fixed", None))
        self.profile_combo_box.setItemText(2, QCoreApplication.translate("SpeedControl", u"Custom", None))
