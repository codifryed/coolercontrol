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
from subprocess import check_call


def lint() -> None:
    check_call(["pylint", "--rcfile=config/pylintrc", "src/main/python/coolero"])
    check_call(["mypy", "--config-file", "config/mypy.ini", "src/main/python", "tests"])


def test() -> None:
    check_call(["pytest", "-c", "config/pytest.ini", "-n", "auto", "-k", "tests"])


def coolero() -> None:
    check_call(["python3", "src/main/python/coolero/coolero.py"])
