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

"""
PySide6-Frameless-Window
========================
A cross-platform frameless window based on pyside6, support Win32, Linux and macOS.

Documentation is available in the docstrings and
online at https://pyqt-frameless-window.readthedocs.io.

Examples are available at https://github.com/zhiyiYo/PyQt-Frameless-Window/tree/PySide6/examples.

:copyright: (c) 2021 by zhiyiYo.
:license: LGPLv3, see LICENSE for more details.
"""

__version__ = "0.1.0"

import sys

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QDialog, QMainWindow

from .titlebar import TitleBar, TitleBarButton, SvgTitleBarButton, StandardTitleBar

from .linux import LinuxFramelessWindow as FramelessWindow
from .linux import LinuxFramelessMainWindow as FramelessMainWindow
from .linux import LinuxFramelessDialog as FramelessDialog
from .linux import LinuxWindowEffect as WindowEffect

AcrylicWindow = FramelessWindow
