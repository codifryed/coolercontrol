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


def build() -> None:
    run(_nuitka_common_build_command() + ["--standalone"], check=True)


def build_one_file() -> None:
    run(_nuitka_common_build_command() + ["--onefile"], check=True)


def _nuitka_common_build_command() -> list[str]:
    return [
        "python3", "-m", "nuitka",
        "--follow-imports",
        "--include-data-dir=./coolero/config=config",
        "--include-data-dir=./coolero/resources=resources",
        "--plugin-enable=anti-bloat,pyside6,pylint-warnings,numpy",
        "--include-module=services.liquidctl_device_extractors",
        "--lto=yes",
        "--prefer-source-code",
        "--python-flag=-S,-O,no_docstrings",
        "--linux-onefile-icon=metadata/org.coolero.Coolero.png",
        "coolero/coolero.py"
    ]


def build_pyinstaller() -> None:
    run(_prepare_pyinstaller_build_command(), check=True)


def build_one_file_pyinstaller() -> None:
    run(_prepare_pyinstaller_build_command(one_file=True), check=True)


def _prepare_pyinstaller_build_command(one_file: bool = False) -> list[str]:
    app_path = Path(__file__).resolve().parent
    extractor_path = [str(app_path.joinpath('services/liquidctl_device_extractors'))]
    auto_imported_subclasses = ['--hidden-import=services.liquidctl_device_extractors.' + module.name
                                for module in pkgutil.iter_modules(extractor_path)]
    one_file_option = ["--onefile"] if one_file else []
    return ["pyinstaller", "-y", "--clean",
            f"--paths={app_path}",
            f"--add-data={app_path.joinpath('resources')}:resources",
            f"--add-data={app_path.joinpath('config')}:config",
            "--hidden-import=PySide6.QtSvg"
            ] + auto_imported_subclasses + one_file_option + [
               f"{app_path.joinpath('coolero.py')}"
           ]


def bump() -> None:
    from .settings import Settings
    if len(sys.argv) < 2 or not sys.argv[1]:
        raise ValueError("version to bump to is not present")
    new_version = sys.argv[1]
    print(f'Setting application version to {new_version}')
    Settings.app["version"] = new_version
    Settings.save_app_settings()
