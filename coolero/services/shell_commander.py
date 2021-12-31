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

import logging
import platform
import subprocess
from pathlib import Path
from subprocess import CompletedProcess, CalledProcessError
from typing import List

from settings import Settings

_LOG = logging.getLogger(__name__)
_LIQUIDCTL_UDEV_RULES_LOCATION: str = 'config/71-liquidctl.rules'
_UDEV_RULES_PATH: Path = Path('/etc/udev/rules.d/')
_UDEV_RELOAD_COMMANDS: str = 'udevadm control --reload-rules && udevadm trigger -w --subsystem-match=usb --action=add'
_APP_IMAGE_UPDATE_COMMAND: List[str] = ['sh', '-c', '$APPDIR/AppImageUpdate $APPIMAGE']
_EXEC_COMMAND: List[str] = ['pkexec', 'sh', '-c']
_FLATPAK_COMMAND_PREFIX = ['flatpak-spawn', '--host']


class ShellCommander:

    @staticmethod
    def apply_udev_rules() -> bool:
        """
        Will attempt to apply udev rules for user access to usb devices and return whether it was successful or not
        """
        if platform.system() != 'Linux':
            return False
        lc_rules_path: Path = Settings.application_path.joinpath(_LIQUIDCTL_UDEV_RULES_LOCATION)
        # todo: there may need to be adjustments to the command if being run from packaging like flatpak and snap
        command = _EXEC_COMMAND + [f'cp -f {lc_rules_path} {_UDEV_RULES_PATH} && {_UDEV_RELOAD_COMMANDS}']
        try:
            completed_command: CompletedProcess = subprocess.run(command, capture_output=True, check=True)
            _LOG.info('UDev rules successfully applied.')
            _LOG.debug('UDev applied rules output: %s', completed_command.stdout)
            return True
        except CalledProcessError as error:
            _LOG.error('Failed to apply udev rules. Error: %s', error.stderr)
            _LOG.debug('Command that failed: %s', error.cmd)
        return False

    @staticmethod
    def run_app_image_update() -> bool:
        if platform.system() != 'Linux':
            return False
        try:
            subprocess.run(_APP_IMAGE_UPDATE_COMMAND, capture_output=False, check=True)
            return True
        except CalledProcessError as error:
            _LOG.error('Failed to run AppImageUpdate. Error: %s', error.stderr)
            _LOG.debug('Command that failed: %s', error.cmd)
        except FileNotFoundError as err:
            _LOG.error('AppImageUpdate Not found', exc_info=err)
        return False
