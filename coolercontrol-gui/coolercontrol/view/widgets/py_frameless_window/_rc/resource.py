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

# Resource object code (Python 3)
# Created by: object code
# Created by: The Resource Compiler for Qt version 6.4.2
# WARNING! All changes made in this file will be lost!

from PySide6 import QtCore

qt_resource_data = b"\
\x00\x00\x01\x0e\
<\
svg width=\x2245pt\x22\
 height=\x2230pt\x22 v\
ersion=\x221.1\x22 vie\
wBox=\x220 0 15.875\
 10.583\x22 xmlns=\x22\
http://www.w3.or\
g/2000/svg\x22>\x0d\x0a <\
g fill=\x22none\x22 st\
roke=\x22#000\x22 stro\
ke-width=\x22.17639\
\x22>\x0d\x0a  <path d=\x22m\
6.1295 3.6601 3.\
2632 3.2632z\x22/>\x0d\
\x0a  <path d=\x22m9.3\
927 3.6601-3.263\
2 3.2632z\x22/>\x0d\x0a <\
/g>\x0d\x0a</svg>\x0d\x0a\
"

qt_resource_name = b"\
\x00\x10\
\x0a\xb5\xe4\x07\
\x00q\
\x00f\x00r\x00a\x00m\x00e\x00l\x00e\x00s\x00s\x00w\x00i\x00n\x00d\x00o\x00w\
\x00\x09\
\x06\x98\x8e\xa7\
\x00c\
\x00l\x00o\x00s\x00e\x00.\x00s\x00v\x00g\
"

qt_resource_struct = b"\
\x00\x00\x00\x00\x00\x02\x00\x00\x00\x01\x00\x00\x00\x01\
\x00\x00\x00\x00\x00\x00\x00\x00\
\x00\x00\x00\x00\x00\x02\x00\x00\x00\x01\x00\x00\x00\x02\
\x00\x00\x00\x00\x00\x00\x00\x00\
\x00\x00\x00&\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\
\x00\x00\x01\x85\xca\xa5\xfb\xd1\
"


def qInitResources():
    QtCore.qRegisterResourceData(0x03, qt_resource_struct, qt_resource_name, qt_resource_data)


def qCleanupResources():
    QtCore.qUnregisterResourceData(0x03, qt_resource_struct, qt_resource_name, qt_resource_data)


qInitResources()
