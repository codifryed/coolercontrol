#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2023  Guy Boldon and zhiyiYo
#  This code has been modified from the original PySide6-Frameless-Window by zhiyiYo.
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

from PySide6.QtCore import QCoreApplication, QEvent, Qt
from PySide6.QtWidgets import QWidget, QMainWindow, QDialog

from ..titlebar import TitleBar
from ..utils.linux_utils import LinuxMoveResize
from .window_effect import LinuxWindowEffect


class LinuxFramelessWindowBase:
    """ Frameless window base class for Linux system """

    BORDER_WIDTH = 5

    def __init__(self, *args, **kwargs):
        pass

    def _initFrameless(self):
        self.windowEffect = LinuxWindowEffect(self)
        self.titleBar = TitleBar(self)
        self._isResizeEnabled = True

        self.setWindowFlags(self.windowFlags() | Qt.FramelessWindowHint)
        QCoreApplication.instance().installEventFilter(self)

        self.titleBar.raise_()
        self.resize(500, 500)

    def resizeEvent(self, e):
        self.titleBar.resize(self.width(), self.titleBar.height())

    def setTitleBar(self, titleBar):
        """ set custom title bar

        Parameters
        ----------
        titleBar: TitleBar
            title bar
        """
        self.titleBar.deleteLater()
        self.titleBar = titleBar
        self.titleBar.setParent(self)
        self.titleBar.raise_()

    def setResizeEnabled(self, isEnabled: bool):
        """ set whether resizing is enabled """
        self._isResizeEnabled = isEnabled

    def eventFilter(self, obj, event):
        et = event.type()
        if (
                et not in [QEvent.MouseButtonPress, QEvent.MouseMove]
                or not self._isResizeEnabled
                or obj is not None and obj.objectName() not in ["pod_bg_app", "MainWindowWindow"]  # only the main window is resizable (bug)
        ):
            return False

        edges = Qt.Edge(0)
        pos = event.globalPos() - self.pos()
        if pos.x() < self.BORDER_WIDTH:
            edges |= Qt.LeftEdge
        if pos.x() >= self.width() - self.BORDER_WIDTH:
            edges |= Qt.RightEdge
        if pos.y() < self.BORDER_WIDTH:
            edges |= Qt.TopEdge
        if pos.y() >= self.height() - self.BORDER_WIDTH:
            edges |= Qt.BottomEdge

        # change cursor
        if et == QEvent.MouseMove and self.windowState() == Qt.WindowNoState:
            if edges in (Qt.LeftEdge | Qt.TopEdge, Qt.RightEdge | Qt.BottomEdge):
                self.setCursor(Qt.SizeFDiagCursor)
            elif edges in (Qt.RightEdge | Qt.TopEdge, Qt.LeftEdge | Qt.BottomEdge):
                self.setCursor(Qt.SizeBDiagCursor)
            elif edges in (Qt.TopEdge, Qt.BottomEdge):
                self.setCursor(Qt.SizeVerCursor)
            elif edges in (Qt.LeftEdge, Qt.RightEdge):
                self.setCursor(Qt.SizeHorCursor)
            else:
                self.setCursor(Qt.ArrowCursor)

        elif obj in (self, self.titleBar) and et == QEvent.MouseButtonPress and edges:
            LinuxMoveResize.starSystemResize(self, event.globalPos(), edges)

        return False


class LinuxFramelessWindow(QWidget, LinuxFramelessWindowBase):
    """ Frameless window for Linux system """

    def __init__(self, parent=None):
        super().__init__(parent=parent)
        self._initFrameless()

    def resizeEvent(self, e):
        LinuxFramelessWindowBase.resizeEvent(self, e)

    def eventFilter(self, obj, event):
        return LinuxFramelessWindowBase.eventFilter(self, obj, event)


class LinuxFramelessMainWindow(QMainWindow, LinuxFramelessWindowBase):
    """ Frameless main window for Linux system """

    def __init__(self, parent=None):
        super().__init__(parent=parent)
        self._initFrameless()

    def resizeEvent(self, e):
        QMainWindow.resizeEvent(self, e)
        self.titleBar.resize(self.width(), self.titleBar.height())

    def eventFilter(self, obj, event):
        return LinuxFramelessWindowBase.eventFilter(self, obj, event)


class LinuxFramelessDialog(QDialog, LinuxFramelessWindowBase):
    """ Frameless dialog for Windows system """

    def __init__(self, parent=None):
        super().__init__(parent=parent)
        self._initFrameless()
        self.titleBar.minBtn.hide()
        self.titleBar.maxBtn.hide()
        self.titleBar.setDoubleClickEnabled(False)

    def resizeEvent(self, e):
        self.titleBar.resize(self.width(), self.titleBar.height())

    def eventFilter(self, obj, event):
        return LinuxFramelessWindowBase.eventFilter(self, obj, event)
