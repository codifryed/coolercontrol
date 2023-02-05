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

import logging
import platform
import subprocess
from subprocess import CompletedProcess, CalledProcessError, TimeoutExpired

log = logging.getLogger(__name__)
_COMMAND_SHELL_PREFIX: list[str] = ['sh', '-c']
_COMMAND_APP_IMAGE_CHECK_UPDATE: list[str] = _COMMAND_SHELL_PREFIX + ['$APPDIR/AppImageUpdate -j $APPIMAGE']
_COMMAND_APP_IMAGE_UPDATE: list[str] = _COMMAND_SHELL_PREFIX + ['$APPDIR/AppImageUpdate $APPIMAGE']


class ShellCommander:

    @staticmethod
    def check_if_app_image_has_update() -> bool:
        if platform.system() != 'Linux':
            return False
        try:
            command_result: CompletedProcess = subprocess.run(
                _COMMAND_APP_IMAGE_CHECK_UPDATE, capture_output=True, check=False, timeout=5.0
            )  # Command exits with:
            # code 1 if changes are available, 0 if there are not, other non-zero code in case of errors.
            if command_result.returncode == 1:
                return True
            if command_result.returncode != 0:
                log.error('Error when checking for AppImage update: %s', command_result.stderr)
            return False
        except TimeoutExpired as exp:
            log.warning('Check for AppImage Update command timed out: %s', exp.stderr)
            return False

    @staticmethod
    def run_app_image_update() -> bool:
        if platform.system() != 'Linux':
            return False
        try:
            subprocess.run(_COMMAND_APP_IMAGE_UPDATE, capture_output=False, check=True)
            return True
        except CalledProcessError as error:
            log.error('Failed to run AppImageUpdate. Error: %s', error.stderr)
            log.debug('Command that failed: %s', error.cmd)
        except FileNotFoundError as err:
            log.error('AppImageUpdate Not found', exc_info=err)
        return False
