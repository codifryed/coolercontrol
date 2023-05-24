#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2023  Guy Boldon
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

from PySide6.QtCore import Qt
from PySide6.QtGui import QResizeEvent
from PySide6.QtWidgets import QColorDialog, QWidget, QDialog, QVBoxLayout, QSpacerItem

from coolercontrol.dialogs.dialog_style import DIALOG_STYLE
from coolercontrol.dialogs.dialog_window import LinuxFramelessDialogWindowBase
from coolercontrol.dialogs.dialog_window_style import DIALOG_WINDOW_STYLE
from coolercontrol.settings import Settings


class ColorDialog(QDialog, LinuxFramelessDialogWindowBase):
    def __init__(self, parent=None) -> None:
        super().__init__(parent=parent)
        self._initFrameless(parent=self)
        self.setObjectName("dialog_window")
        self.setLayout(QVBoxLayout())
        self.setContentsMargins(10, 10, 10, 10)
        self.window_frame = QWidget()
        self.window_frame.setObjectName("window_frame")
        self.window_frame.setAttribute(Qt.WA_TranslucentBackground)
        self.window_frame.setWindowFlags(Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint)
        self.setParent(self.window_frame)
        self.setWindowFlag(Qt.WindowStaysOnTopHint)
        self.color_picker = QColorDialog()
        self.color_picker.setWindowFlags(self.color_picker.windowFlags() & ~Qt.Dialog)
        self.layout().addItem(QSpacerItem(1,20))
        self.layout().addWidget(self.color_picker)
        self.setFixedWidth(590)
        self.setFixedHeight(490)
        self.setWindowTitle('Select LED Color')

        # AppImage for won't use the native dialog, the Qt one works very well IMHO and allows us to use same UI colors.
        self.color_picker.setOption(QColorDialog.DontUseNativeDialog, True)
        self.setStyleSheet(DIALOG_STYLE.format(
            _text_size=Settings.app["font"]["text_size"],
            _font_family=Settings.app["font"]["family"],
            _text_color=Settings.theme["app_color"]["text_foreground"],
            _bg_color=Settings.theme["app_color"]["bg_three"]
        ))
        self.color_picker.setStyleSheet(DIALOG_STYLE.format(
            _text_size=Settings.app["font"]["text_size"],
            _font_family=Settings.app["font"]["family"],
            _text_color=Settings.theme["app_color"]["text_title"],
            _bg_color=Settings.theme["app_color"]["bg_three"]
        ))
        self.setStyleSheet(DIALOG_WINDOW_STYLE.format(
            _text_size=Settings.app["font"]["text_size"],
            _font_family=Settings.app["font"]["family"],
            _text_color=Settings.theme["app_color"]["text_foreground"],
            _bg_color=Settings.theme["app_color"]["bg_three"]
        ))

        # helpful standard custom colors:
        self.color_picker.setCustomColor(0, Qt.red)
        self.color_picker.setCustomColor(1, Qt.green)
        self.color_picker.setCustomColor(2, Qt.blue)
        self.color_picker.setCustomColor(3, Qt.yellow)
        self.color_picker.setCustomColor(4, Qt.cyan)
        self.color_picker.setCustomColor(5, Qt.magenta)


    def resizeEvent(self, event: QResizeEvent) -> None:
        self.titleBar.resize(self.width(), self.titleBar.height())
        self.move(0, 0)  # this fixes a placement issue on x11

    def display(self) -> int:
        self.window_frame.show()
        result = self.color_picker.exec()
        self.color_picker.close()
        self.window_frame.close()
        return result
