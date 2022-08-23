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
import logging
import os
import platform
from pathlib import Path

import colorlog
import setproctitle

from server import Server, SOCKET_NAME

log = logging.getLogger(__name__)
VERSION: str = '0.1.0'
SYSTEM_RUN_PATH: Path = Path("run").joinpath("coolercontrol")


def main() -> None:
    setproctitle.setproctitle("coolercontrol-liqctld")
    parser = argparse.ArgumentParser(
        description='a daemon service for liquidctl',
        exit_on_error=False
    )
    parser.add_argument('-v', '--version', action='version', version=f'Liqctld v{VERSION} - {system_info()}')
    parser.add_argument('--debug', action='store_true', help='turn on debug logging')
    args = parser.parse_args()
    if args.debug:
        log_level = logging.DEBUG
        liquidctl_level = logging.DEBUG
    else:
        log_level = logging.INFO
        liquidctl_level = logging.WARNING

    is_systemd: bool = SYSTEM_RUN_PATH.joinpath(SOCKET_NAME).exists() and os.geteuid() == 0
    if is_systemd:
        log_format = "%(log_color)s%(levelname)s: %(name)s - %(message)s"
    else:
        log_format = "%(log_color)s%(asctime)s %(levelname)s: %(name)s - %(message)s"

    handler = colorlog.StreamHandler()
    handler.setFormatter(colorlog.ColoredFormatter(log_format))
    root_logger = colorlog.getLogger('root')
    root_logger.setLevel(log_level)
    root_logger.addHandler(handler)
    liquidctl_logger = colorlog.getLogger('liquidctl')
    liquidctl_logger.setLevel(liquidctl_level)

    log.info("Liquidctl daemon initializing")
    if args.debug:
        log.debug("DEBUG level enabled")
        log.debug(system_info())
    Server(is_systemd)


def system_info() -> str:
    sys_info = f'System Info: Python: v{platform.python_version()} OS: {platform.platform()}'
    if platform.system() == 'Linux':
        sys_info = f'{sys_info} Dist: {platform.freedesktop_os_release()["PRETTY_NAME"]}'
    return sys_info


if __name__ == "__main__":
    main()
