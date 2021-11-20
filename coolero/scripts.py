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
import pkgutil
import sys
from pathlib import Path
from subprocess import run


def lint() -> None:
    run(["pylint", "--rcfile=coolero/config/pylintrc", "coolero"], check=True)
    run(["mypy", "--config-file", "coolero/config/mypy.ini", "coolero", "tests"], check=True)


def test() -> None:
    run(["pytest", "-c", "coolero/config/pytest.ini", "-n", "auto", "-k", "tests"], check=True)


def coolero() -> None:
    run(["python3", "coolero/coolero.py"] + sys.argv[1:], check=True)


def build_nuitka() -> None:
    # coming with a bug fix in the future
    run(["python3", "-m", "nuitka",
         "--follow-imports",
         "--standalone",
         "--include-data-dir=./coolero/config=config",
         "--include-data-dir=./coolero/resources=resources",
         "--plugin-enable=pyside6", "--plugin-enable=pylint-warnings", "--plugin-enable=numpy",
         "coolero/coolero.py"],
        check=True
        )


def build() -> None:
    app_path = Path(__file__).resolve().parent
    extractor_path = [str(app_path.joinpath('services/liquidctl_device_extractors'))]
    auto_imported_subclasses = ['--hidden-import=services.liquidctl_device_extractors.' + module.name
                                for module in pkgutil.iter_modules(extractor_path)]
    run([
            "pyinstaller", "-y", "--clean",
            f"--paths={app_path}",
            f"--add-data={app_path.joinpath('resources')}:resources",
            f"--add-data={app_path.joinpath('config')}:config",
            "--hidden-import=PySide6.QtSvg"
        ] + auto_imported_subclasses + [
            # "--onefile",
            f"{app_path.joinpath('coolero.py')}"
        ],
        check=True)
