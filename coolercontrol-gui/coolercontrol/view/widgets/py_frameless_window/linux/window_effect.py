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

class LinuxWindowEffect:
    """ Linux window effect """

    def __init__(self, window):
        self.window = window

    def setAcrylicEffect(self, hWnd, gradientColor="F2F2F230", isEnableShadow=True, animationId=0):
        """ set acrylic effect for window

        Parameter
        ----------
        hWnd: int or `sip.voidptr`
            window handle

        gradientColor: str
            hexadecimal acrylic mixed color, corresponding to RGBA components

        isEnableShadow: bool
            whether to enable window shadow

        animationId: int
            turn on blur animation or not
        """
        pass

    def setMicaEffect(self, hWnd):
        """ Add mica effect to the window (Win11 only)

        Parameters
        ----------
        hWnd: int or `sip.voidptr`
            Window handle
        """
        pass

    def setAeroEffect(self, hWnd):
        """ add Aero effect to the window

        Parameter
        ----------
        hWnd: int or `sip.voidptr`
            Window handle
        """
        pass

    def setTransparentEffect(self, hWnd):
        """ set transparent effect for window

        Parameters
        ----------
        hWnd : int or `sip.voidptr`
            Window handle
        """
        pass

    def removeBackgroundEffect(self, hWnd):
        """ Remove background effect

        Parameters
        ----------
        hWnd : int or `sip.voidptr`
            Window handle
        """
        pass

    def addShadowEffect(self, hWnd):
        """ add shadow to window

        Parameter
        ----------
        hWnd: int or `sip.voidptr`
            Window handle
        """
        pass

    def addMenuShadowEffect(self, hWnd):
        """ add shadow to menu

        Parameter
        ----------
        hWnd: int or `sip.voidptr`
            Window handle
        """
        pass

    @staticmethod
    def removeMenuShadowEffect(hWnd):
        """ Remove shadow from pop-up menu

        Parameters
        ----------
        hWnd: int or `sip.voidptr`
            Window handle
        """
        pass

    def removeShadowEffect(self, hWnd):
        """ Remove shadow from the window

        Parameters
        ----------
        hWnd: int or `sip.voidptr`
            Window handle
        """
        pass

    @staticmethod
    def addWindowAnimation(hWnd):
        """ Enables the maximize and minimize animation of the window

        Parameters
        ----------
        hWnd : int or `sip.voidptr`
            Window handle
        """
        pass

    def enableBlurBehindWindow(self, hWnd):
        """ enable the blur effect behind the whole client
        Parameters
        ----------
        hWnd: int or `sip.voidptr`
            Window handle
        """
        pass
