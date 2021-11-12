#  Coolero - monitor and control your cooling and other devices
#  Copyright (c) 2021  Guy Boldon
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
import sys
from subprocess import run


def lint() -> None:
    run(["pylint", "--rcfile=coolero/config/pylintrc", "coolero"])
    run(["mypy", "--config-file", "coolero/config/mypy.ini", "coolero", "tests"])


def test() -> None:
    run(["pytest", "-c", "coolero/config/pytest.ini", "-n", "auto", "-k", "tests"])


def coolero() -> None:
    run(["python3", "coolero/coolero.py"] + sys.argv[1:])


def build() -> None:
    run(["python3", "-m", "nuitka",
         "--follow-imports",
         "--standalone",
         "--include-data-dir=./coolero/config=config",
         "--include-data-dir=./coolero/resources=resources",
         "--plugin-enable=pyside6", "--plugin-enable=pylint-warnings", "--plugin-enable=numpy",
         "coolero/coolero.py"]
        )
