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
from subprocess import CompletedProcess, CalledProcessError, TimeoutExpired
from typing import List, Optional

from models.status_nvidia import StatusNvidia
from settings import Settings, IS_FLATPAK

_LOG = logging.getLogger(__name__)
_LOCATION_LIQUIDCTL_UDEV_RULES: str = 'config/71-liquidctl.rules'
_PATH_UDEV_RULES: Path = Path('/etc/udev/rules.d/')
_COMMAND_FLATPAK_PREFIX: List[str] = ['flatpak-spawn', '--host']
_COMMAND_PKEXEC_PREFIX: List[str] = ['pkexec', 'sh', '-c']
_COMMAND_UDEV_RELOAD: str = 'udevadm control --reload-rules && udevadm trigger -w --subsystem-match=usb --action=add'
_COMMAND_APP_IMAGE_CHECK_UPDATE: List[str] = ['sh', '-c', '$APPDIR/AppImageUpdate -j $APPIMAGE']
_COMMAND_APP_IMAGE_UPDATE: List[str] = ['sh', '-c', '$APPDIR/AppImageUpdate $APPIMAGE']
_COMMAND_NVIDIA_SMI: List[str] = [
    'sh', '-c',
    'nvidia-smi --query-gpu=index,gpu_name,temperature.gpu,utilization.gpu,fan.speed --format=csv,noheader,nounits'
]


class ShellCommander:

    @staticmethod
    def apply_udev_rules() -> bool:
        """
        Will attempt to apply udev rules for user access to usb devices and return whether it was successful or not
        """
        if platform.system() != 'Linux':
            return False
        lc_rules_path: Path = Settings.application_path.joinpath(_LOCATION_LIQUIDCTL_UDEV_RULES)
        command = _COMMAND_PKEXEC_PREFIX + [f'cp -f {lc_rules_path} {_PATH_UDEV_RULES} && {_COMMAND_UDEV_RELOAD}']
        if IS_FLATPAK:
            command = _COMMAND_FLATPAK_PREFIX + command
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
                _LOG.error('Error when checking for AppImage update: %s', command_result.stderr)
            return False
        except TimeoutExpired as exp:
            _LOG.warning('Check for AppImage Update command timed out: %s', exp.stderr)
            return False

    @staticmethod
    def run_app_image_update() -> bool:
        if platform.system() != 'Linux':
            return False
        try:
            subprocess.run(_COMMAND_APP_IMAGE_UPDATE, capture_output=False, check=True)
            return True
        except CalledProcessError as error:
            _LOG.error('Failed to run AppImageUpdate. Error: %s', error.stderr)
            _LOG.debug('Command that failed: %s', error.cmd)
        except FileNotFoundError as err:
            _LOG.error('AppImageUpdate Not found', exc_info=err)
        return False

    @staticmethod
    def get_nvidia_status() -> List[StatusNvidia]:
        if platform.system() != 'Linux':
            return []
        command = _COMMAND_NVIDIA_SMI if not IS_FLATPAK else _COMMAND_FLATPAK_PREFIX + _COMMAND_NVIDIA_SMI
        try:
            command_result: CompletedProcess = subprocess.run(command, capture_output=True, check=True, text=True)
        except CalledProcessError:
            _LOG.warning('Nvidia driver not found')
            return []
        try:
            nvidia_gpu_statuses: List[StatusNvidia] = []
            output_lines = str(command_result.stdout).splitlines()
            _LOG.debug('Nvidia raw status output: %s', output_lines)
            for line in output_lines:
                if not line.strip():
                    continue  # skip any empty lines
                values = line.split(', ')
                nvidia_gpu_statuses.append(
                    StatusNvidia(
                        index=int(values[0]),
                        name=str(values[1]),
                        temp=ShellCommander._safe_cast(values[2]),
                        load=ShellCommander._safe_cast(values[3]),
                        fan_duty=ShellCommander._safe_cast(values[4])
                    ))
            return nvidia_gpu_statuses
        except BaseException as err:
            _LOG.error('Nvidia status parsing error', exc_info=err)
            return []

    @staticmethod
    def _safe_cast(value: str) -> Optional[int]:
        try:
            return int(value)
        except ValueError:
            return None
