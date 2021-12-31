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

from __future__ import annotations

import asyncio
import logging
from typing import Optional, TYPE_CHECKING

import aiohttp
from PySide6.QtWidgets import QMessageBox

from dialogs.update_dialog import UpdateDialog
from services.shell_commander import ShellCommander
from settings import Settings

if TYPE_CHECKING:
    from coolero import Initialize  # type: ignore[attr-defined]

_LOG = logging.getLogger(__name__)
_GITLAB_PROJECT_ID: int = 30707566


class AppUpdater:

    @staticmethod
    def run(parent: Initialize) -> None:
        _LOG.info('Checking GitLab for latest release version')
        loop = asyncio.get_event_loop()
        latest_version = loop.run_until_complete(AppUpdater._request_latest_version())
        if latest_version is not None:
            if latest_version == Settings.app['version']:
                _LOG.info('Already running on the latest release version: v%s', latest_version)
            else:
                _LOG.info('Newer version v%s is available, attempting to update', latest_version)
                answer: int = UpdateDialog(latest_version).run()
                if answer == QMessageBox.Yes:
                    successful_update = ShellCommander.run_app_image_update()
                    if successful_update:
                        _LOG.info('Coolero was updated. Quiting to load the updated appimage.')
                        parent.close()

    @staticmethod
    async def _request_latest_version() -> Optional[str]:
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(
                        f'https://gitlab.com/api/v4/projects/{_GITLAB_PROJECT_ID}/releases/',
                        timeout=5
                ) as response:
                    if response.status == 200:
                        json_response = await response.json()
                        try:
                            return str(json_response[0]['tag_name'])
                        except BaseException as err:
                            _LOG.error('Invalid JSON format when requesting latest versions of Coolero', exc_info=err)
        except aiohttp.ClientError as err:
            _LOG.error('Error requesting current version of Coolero', exc_info=err)
        return None
