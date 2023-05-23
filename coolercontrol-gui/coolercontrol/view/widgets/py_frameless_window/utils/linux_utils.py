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

from PySide6.QtCore import QEvent, QPoint, Qt
from PySide6.QtGui import QMouseEvent
from PySide6.QtWidgets import QApplication


class LinuxMoveResize:
    """ Tool class for moving and resizing window """

    @classmethod
    def startSystemMove(cls, window, globalPos):
        """ move window

        Parameters
        ----------
        window: QWidget
            window

        globalPos: QPoint
            the global point of mouse release event
        """
        window.windowHandle().startSystemMove()
        event = QMouseEvent(QEvent.MouseButtonRelease, QPoint(-1, -1),
                            Qt.LeftButton, Qt.NoButton, Qt.NoModifier)
        QApplication.instance().postEvent(window.windowHandle(), event)

    @classmethod
    def starSystemResize(cls, window, globalPos, edges):
        """ resize window

        Parameters
        ----------
        window: QWidget
            window

        globalPos: QPoint
            the global point of mouse release event

        edges: `Qt.Edges`
            window edges
        """
        if not edges:
            return

        window.windowHandle().startSystemResize(edges)
