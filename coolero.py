#!/usr/bin/env python3

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

# NOTE: This file is a placeholder for easy script execution of Coolero and to help building with Nuitka

# nuitka-project: --standalone
# nuitka-project: --follow-imports
# nuitka-project: --include-data-dir=coolero/config=coolero_data/config
# nuitka-project: --include-data-dir=coolero/resources=coolero_data/resources
# nuitka-project: --plugin-enable=anti-bloat,pyside6,pylint-warnings,numpy
# nuitka-project: --include-module=coolero.services.liquidctl_device_extractors
# nuitka-project: --static-libpython=no
# nuitka-project: --lto=no
# nuitka-project: --prefer-source-code
# nuitka-project: --python-flag=-S,-O,no_docstrings
# nuitka-project: --linux-onefile-icon=metadata/org.coolero.Coolero.png

from coolero.app import main

if __name__ == "__main__":
    main()
