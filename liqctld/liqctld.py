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
import platform

log = logging.getLogger(__name__)
_VERSION: str = '0.1.0'


def main() -> None:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s: %(name)s - %(message)s")
    log.info("Liquidctl daemon initializing")
    parser = argparse.ArgumentParser(
        description='a daemon service for liquidctl',
        exit_on_error=False
    )
    parser.add_argument('-v', '--version', action='version', version=f'Liqctld v{_VERSION} {_system_info()}')
    parser.add_argument('--debug', action='store_true', help='turn on debug logging')
    args = parser.parse_args()
    if args.debug:
        logging.getLogger('root').setLevel(logging.DEBUG)
        # logging.getLogger('apscheduler').setLevel(logging.INFO)
        logging.getLogger('liquidctl').setLevel(logging.DEBUG)
        log.debug('DEBUG level enabled %s', _system_info())


def _system_info() -> str:
    sys_info = f'- System Info: Python: v{platform.python_version()} OS: {platform.platform()}'
    if platform.system() == 'Linux':
        sys_info = f'{sys_info} Dist: {platform.freedesktop_os_release()["PRETTY_NAME"]}'
    return sys_info


if __name__ == "__main__":
    main()
