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

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from PySide6.QtWidgets import QMessageBox

from coolero.dialogs.update_dialog import UpdateDialog
from coolero.services.shell_commander import ShellCommander
from coolero.settings import Settings, FeatureToggle

if TYPE_CHECKING:
    from coolero.app import Initialize

_LOG = logging.getLogger(__name__)
_GITLAB_PROJECT_ID: int = 30707566


class AppUpdater:

    @staticmethod
    def run(parent: Initialize) -> None:
        _LOG.info('Checking for newer AppImage')
        has_update: bool = ShellCommander.check_if_app_image_has_update()
        if not has_update and not FeatureToggle.testing:
            _LOG.info('Already running on the latest release version: v%s', Settings.app['version'])
        else:
            _LOG.info('Update is available, attempting to download changes and update.')
            answer: int = UpdateDialog().run()
            if answer == QMessageBox.Yes:
                successful_update = ShellCommander.run_app_image_update()
                if successful_update:
                    _LOG.info('Coolero was updated. Quiting to load the updated AppImage.')
                    parent.close()
