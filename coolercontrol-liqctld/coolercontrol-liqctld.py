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

# nuitka-project: --standalone
# nuitka-project: --follow-imports
# nuitka-project: --static-libpython=yes
# nuitka-project: --lto=no
# nuitka-project: --prefer-source-code
# nuitka-project: --python-flag=-S,-O,no_docstrings

import argparse
import importlib.metadata
import logging
import os
import platform
import textwrap

import colorlog
import setproctitle

from server import Server


def add_log_level() -> None:
    debug_lc_lvl: int = 15

    def log_for_level(self, message, *args, **kwargs) -> None:
        if self.isEnabledFor(debug_lc_lvl):
            self._log(debug_lc_lvl, message, args, **kwargs)

    def log_to_root(message, *args, **kwargs) -> None:
        logging.log(debug_lc_lvl, message, *args, **kwargs)

    logging.addLevelName(debug_lc_lvl, 'DEBUG_LC')
    setattr(logging, 'DEBUG_LC', debug_lc_lvl)
    setattr(logging, 'debug_lc', log_to_root)
    setattr(logging.getLoggerClass(), 'debug_lc', log_for_level)


add_log_level()
log = logging.getLogger(__name__)
__version__: str = '0.14.0'


def main() -> None:
    setproctitle.setproctitle("coolercontrol-liqctld")
    parser = argparse.ArgumentParser(
        description='a daemon service for liquidctl',
        exit_on_error=False,
        formatter_class=argparse.RawTextHelpFormatter
    )
    parser.add_argument("-v", "--version", action="version", version=f"\n {system_info()}")
    parser.add_argument("--debug", action="store_true", help="enable debug output \n")
    parser.add_argument("--debug-liquidctl", action="store_true", help="enable liquidctl debug output\n")
    parser.add_argument("-d", "--daemon", action="store_true", help="Starts liqctld in Systemd daemon mode")
    args = parser.parse_args()
    if args.debug:
        log_level = logging.DEBUG
        liquidctl_level = logging.DEBUG
        uvicorn_level = logging.DEBUG
    elif args.debug_liquidctl:
        log_level = logging.DEBUG_LC
        liquidctl_level = logging.DEBUG
        uvicorn_level = logging.WARNING
    else:
        log_level = logging.INFO
        liquidctl_level = logging.WARNING
        uvicorn_level = logging.WARNING

    is_systemd: bool = args.daemon and os.geteuid() == 0
    if is_systemd:
        log_format = "%(log_color)s%(levelname)-8s %(name)s - %(message)s"
    else:
        log_format = "%(log_color)s%(asctime)-15s %(levelname)-8s %(name)s - %(message)s"

    handler = colorlog.StreamHandler()
    handler.setFormatter(colorlog.ColoredFormatter(log_format))
    root_logger = colorlog.getLogger('root')
    root_logger.setLevel(log_level)
    root_logger.addHandler(handler)
    liquidctl_logger = colorlog.getLogger('liquidctl')
    liquidctl_logger.setLevel(liquidctl_level)

    log.info("Liquidctl daemon initializing")
    if args.debug:
        log.debug('DEBUG level enabled\n%s', system_info())
    elif args.debug_liquidctl:
        log.debug_lc('Liquidctl DEBUG_LC level enabled\n%s', system_info())

    server = Server(__version__, is_systemd, uvicorn_level)
    server.startup()


def system_info() -> str:
    sys_info: str = textwrap.dedent(f'''
            Liqctld v{__version__}

            System:''')
    if platform.system() == 'Linux':
        sys_info += f'\n    {platform.freedesktop_os_release().get("PRETTY_NAME")}'  # type: ignore
    sys_info += textwrap.dedent(f'''
                {platform.platform()}

            Dependency versions:
                Python     {platform.python_version()}
                Liquidctl  {_get_package_version("liquidctl")}
                Hidapi     {_get_package_version("hidapi")}
                Pyusb      {_get_package_version("pyusb")}
                Pillow     {_get_package_version("pillow")}
                Smbus      {_get_package_version("smbus")}
            ''')
    return sys_info


def _get_package_version(package_name: str) -> str:
    """This searches for package versions.
    First it checks the metadata, which is present for all packages.
    If the metadata isn't found, like with the compiled AppImage, it checks inside the package for __version__.
    If package doesn't exist then it either defaults to the last known version or "unknown"
    """
    try:
        return importlib.metadata.version(package_name)
    except importlib.metadata.PackageNotFoundError:
        match package_name:
            case "liquidctl":
                import liquidctl
                return _get_version_attribute(liquidctl)
            case "hidapi":
                return ">=0.12.0.post2"
            case "pyusb":
                return ">=1.2.1"
            case "pillow":
                import PIL
                return _get_version_attribute(PIL)
            case "smbus":
                return ">=1.1.post2"
            case _:
                return "unknown"


def _get_version_attribute(package_object: object) -> str:
    return getattr(package_object, "__version__", "unknown")


if __name__ == "__main__":
    main()
