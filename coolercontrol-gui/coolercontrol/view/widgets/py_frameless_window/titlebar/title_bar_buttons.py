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

from enum import Enum

from PySide6.QtCore import QFile, QPointF, QRectF, Qt, Property
from PySide6.QtGui import QColor, QPainter, QPainterPath, QPen
from PySide6.QtWidgets import QAbstractButton
from PySide6.QtSvg import QSvgRenderer
from PySide6.QtXml import QDomDocument

from .._rc import resource


class TitleBarButtonState(Enum):
    """ Title bar button state """
    NORMAL = 0
    HOVER = 1
    PRESSED = 2


class TitleBarButton(QAbstractButton):
    """ Title bar button """

    def __init__(self, parent=None):
        super().__init__(parent=parent)
        self.setCursor(Qt.ArrowCursor)
        self.setFixedSize(46, 32)
        self._state = TitleBarButtonState.NORMAL

        # icon color
        self._normalColor = QColor(0, 0, 0)
        self._hoverColor = QColor(0, 0, 0)
        self._pressedColor = QColor(0, 0, 0)

        # background color
        self._normalBgColor = QColor(0, 0, 0, 0)
        self._hoverBgColor = QColor(0, 0, 0, 26)
        self._pressedBgColor = QColor(0, 0, 0, 51)
        self.bg_radius: int = 14

    def setState(self, state):
        """ set the state of button

        Parameters
        ----------
        state: TitleBarButtonState
            the state of button
        """
        self._state = state
        self.update()

    def isPressed(self):
        """ whether the button is pressed """
        return self._state == TitleBarButtonState.PRESSED

    def getNormalColor(self):
        """ get the icon color of the button in normal state """
        return self._normalColor

    def getHoverColor(self):
        """ get the icon color of the button in hover state """
        return self._hoverColor

    def getPressedColor(self):
        """ get the icon color of the button in pressed state """
        return self._pressedColor

    def getNormalBackgroundColor(self):
        """ get the background color of the button in normal state """
        return self._normalBgColor

    def getHoverBackgroundColor(self):
        """ get the background color of the button in hover state """
        return self._hoverBgColor

    def getPressedBackgroundColor(self):
        """ get the background color of the button in pressed state """
        return self._pressedBgColor

    def setNormalColor(self, color):
        """ set the icon color of the button in normal state

        Parameters
        ----------
        color: QColor
            icon color
        """
        self._normalColor = QColor(color)
        self.update()

    def setHoverColor(self, color):
        """ set the icon color of the button in hover state

        Parameters
        ----------
        color: QColor
            icon color
        """
        self._hoverColor = QColor(color)
        self.update()

    def setPressedColor(self, color):
        """ set the icon color of the button in pressed state

        Parameters
        ----------
        color: QColor
            icon color
        """
        self._pressedColor = QColor(color)
        self.update()

    def setNormalBackgroundColor(self, color):
        """ set the background color of the button in normal state

        Parameters
        ----------
        color: QColor
            background color
        """
        self._normalBgColor = QColor(color)
        self.update()

    def setHoverBackgroundColor(self, color):
        """ set the background color of the button in hover state

        Parameters
        ----------
        color: QColor
            background color
        """
        self._hoverBgColor = QColor(color)
        self.update()

    def setPressedBackgroundColor(self, color):
        """ set the background color of the button in pressed state

        Parameters
        ----------
        color: QColor
            background color
        """
        self._pressedBgColor = QColor(color)
        self.update()

    def enterEvent(self, e):
        self.setState(TitleBarButtonState.HOVER)
        super().enterEvent(e)

    def leaveEvent(self, e):
        self.setState(TitleBarButtonState.NORMAL)
        super().leaveEvent(e)

    def mousePressEvent(self, e):
        if e.button() != Qt.LeftButton:
            return

        self.setState(TitleBarButtonState.PRESSED)
        super().mousePressEvent(e)

    def _getColors(self):
        """ get the icon color and background color """
        if self._state == TitleBarButtonState.NORMAL:
            return self._normalColor, self._normalBgColor
        elif self._state == TitleBarButtonState.HOVER:
            return self._hoverColor, self._hoverBgColor

        return self._pressedColor, self._pressedBgColor

    normalColor = Property(QColor, getNormalColor, setNormalColor)
    hoverColor = Property(QColor, getHoverColor, setHoverColor)
    pressedColor = Property(QColor, getPressedColor, setPressedColor)
    normalBackgroundColor = Property(
        QColor, getNormalBackgroundColor, setNormalBackgroundColor)
    hoverBackgroundColor = Property(
        QColor, getHoverBackgroundColor, setHoverBackgroundColor)
    pressedBackgroundColor = Property(
        QColor, getPressedBackgroundColor, setPressedBackgroundColor)


class SvgTitleBarButton(TitleBarButton):
    """ Title bar button using svg icon """

    def __init__(self, iconPath, parent=None):
        """
        Parameters
        ----------
        iconPath: str
            the path of icon

        parent: QWidget
            parent widget
        """
        super().__init__(parent)
        self._svgDom = QDomDocument()
        self.setIcon(iconPath)
        self.bg_radius: int = 14

    def setIcon(self, iconPath):
        """ set the icon of button

        Parameters
        ----------
        iconPath: str
            the path of icon
        """
        f = QFile(iconPath)
        f.open(QFile.ReadOnly)
        self._svgDom.setContent(f.readAll())
        f.close()

    def paintEvent(self, e):
        painter = QPainter(self)
        painter.setRenderHints(QPainter.Antialiasing | QPainter.SmoothPixmapTransform)
        color, bgColor = self._getColors()

        # draw background
        painter.setBrush(bgColor)
        painter.setPen(Qt.NoPen)
        # painter.drawRect(self.rect())
        painter.drawRoundedRect(self.rect(), self.bg_radius, self.bg_radius)

        # draw icon
        color = color.name()
        pathNodes = self._svgDom.elementsByTagName('path')
        for i in range(pathNodes.length()):
            element = pathNodes.at(i).toElement()
            element.setAttribute('stroke', color)

        renderer = QSvgRenderer(self._svgDom.toByteArray())
        renderer.render(painter, QRectF(self.rect()))


class MinimizeButton(TitleBarButton):
    """ Minimize button """

    def paintEvent(self, e):
        painter = QPainter(self)
        color, bgColor = self._getColors()

        # draw background
        painter.setBrush(bgColor)
        painter.setPen(Qt.NoPen)
        # painter.drawRect(self.rect())
        painter.drawRoundedRect(self.rect(), self.bg_radius, self.bg_radius)

        # draw icon
        painter.setBrush(Qt.NoBrush)
        pen = QPen(color, 1)
        pen.setCosmetic(True)
        painter.setPen(pen)
        painter.drawLine(18, 16, 28, 16)


class MaximizeButton(TitleBarButton):
    """ Maximize button """

    def __init__(self, parent=None):
        super().__init__(parent)
        self._isMax = False

    def setMaxState(self, isMax):
        """ update the maximized state and icon """
        if self._isMax == isMax:
            return

        self._isMax = isMax
        self.setState(TitleBarButtonState.NORMAL)

    def paintEvent(self, e):
        painter = QPainter(self)
        color, bgColor = self._getColors()

        # draw background
        painter.setBrush(bgColor)
        painter.setPen(Qt.NoPen)
        # painter.drawRect(self.rect())
        painter.drawRoundedRect(self.rect(), self.bg_radius, self.bg_radius)

        # draw icon
        painter.setBrush(Qt.NoBrush)
        pen = QPen(color, 1)
        pen.setCosmetic(True)
        painter.setPen(pen)

        r = self.devicePixelRatioF()
        painter.scale(1 / r, 1 / r)
        if not self._isMax:
            painter.drawRect(int(18 * r), int(11 * r), int(10 * r), int(10 * r))
        else:
            painter.drawRect(int(18 * r), int(13 * r), int(8 * r), int(8 * r))
            x0 = int(18 * r) + int(2 * r)
            y0 = 13 * r
            dw = int(2 * r)
            path = QPainterPath(QPointF(x0, y0))
            path.lineTo(x0, y0 - dw)
            path.lineTo(x0 + 8 * r, y0 - dw)
            path.lineTo(x0 + 8 * r, y0 - dw + 8 * r)
            path.lineTo(x0 + 8 * r - dw, y0 - dw + 8 * r)
            painter.drawPath(path)


class CloseButton(SvgTitleBarButton):
    """ Close button """

    def __init__(self, parent=None):
        super().__init__(":/qframelesswindow/close.svg", parent)
        self.setHoverColor(Qt.white)
        self.setPressedColor(Qt.white)
        self.setHoverBackgroundColor(QColor(232, 17, 35))
        self.setPressedBackgroundColor(QColor(241, 112, 122))
