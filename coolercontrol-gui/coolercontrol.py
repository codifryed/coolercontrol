#  CoolerControl - monitor and control your cooling and other devices
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

# NOTE: This file is a placeholder for easy script execution of CoolerControl and to help building with Nuitka

# nuitka-project: --standalone
# nuitka-project: --follow-imports
# nuitka-project: --include-data-dir=coolercontrol/config=coolercontrol_data/config
# nuitka-project: --include-data-dir=coolercontrol/resources=coolercontrol_data/resources
# nuitka-project: --plugin-enable=pyside6,pylint-warnings
# nuitka-project: --static-libpython=yes
# nuitka-project: --lto=no
# nuitka-project: --prefer-source-code
# nuitka-project: --python-flag=-S,-O,no_docstrings
# nuitka-project: --linux-onefile-icon=metadata/org.coolercontrol.CoolerControl.png

from coolercontrol.coolercontrol import main

if __name__ == "__main__":
    main()
