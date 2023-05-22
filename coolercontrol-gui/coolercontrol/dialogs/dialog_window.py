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

from coolercontrol.settings import Settings
from coolercontrol.view.widgets.py_frameless_window import StandardTitleBar


class CustomDialogTitleBar(StandardTitleBar):
    """ Custom title bar """

    def __init__(self, parent, icon_w_h: int = 0):
        super().__init__(parent, icon_w_h)
        self.titleLabel.setStyleSheet(
            f'font: {Settings.app["font"]["title_size"]}pt "{Settings.app["font"]["family"]}"; '
            f'color: {Settings.theme["app_color"]["text_title"]};'
            'background: transparent;'
        )
        self.minBtn.hide()
        self.maxBtn.hide()
        self.setDoubleClickEnabled(False)
        self.closeBtn.setNormalColor(Settings.theme["app_color"]["icon_color"])
        self.closeBtn.setHoverColor(Settings.theme["app_color"]["icon_hover"])
        self.closeBtn.setHoverBackgroundColor(Settings.theme["app_color"]["red"])
        self.closeBtn.setPressedColor(Settings.theme["app_color"]["icon_pressed"])
        self.setFixedHeight(42)
        self.hBoxLayout.setContentsMargins(0, 0, 2, 0)


class LinuxFramelessDialogWindowBase:
    """This is a custom version of the LinuxFramelessWindowBase"""

    BORDER_WIDTH = 5

    def __init__(self, *args, **kwargs):
        pass

    def _initFrameless(self, parent=None):
        self.titleBar = CustomDialogTitleBar(parent=parent)
        self._isResizeEnabled = False
        self.setWindowFlags(self.windowFlags() | Qt.FramelessWindowHint)
        self.titleBar.raise_()

    def resizeEvent(self, e):
        self.titleBar.resize(self.width(), self.titleBar.height())
