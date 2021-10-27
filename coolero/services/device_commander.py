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
from typing import List, Tuple

from models.settings import Settings, Setting
from models.speed_profile import SpeedProfile
from repositories.liquidctl_repo import LiquidctlRepo
from view.uis.canvases.speed_control_canvas import SpeedControlCanvas

_LOG = logging.getLogger(__name__)


class DeviceCommander:
    _lc_repo: LiquidctlRepo

    def __init__(self, lc_repo: LiquidctlRepo) -> None:
        self._lc_repo = lc_repo

    def set_speed(self, subject: SpeedControlCanvas) -> None:
        channel: str = subject.channel_name
        device_id: int = subject.device.lc_device_id
        if subject.current_speed_profile == SpeedProfile.FIXED:
            setting = Setting(speed_fixed=subject.fixed_duty)
        elif subject.current_speed_profile == SpeedProfile.CUSTOM:
            setting = Setting(
                speed_profile=self._convert_axis_to_profile(subject.profile_temps, subject.profile_duties))
        else:
            setting = Setting()
        settings = Settings({channel: setting})
        _LOG.info('Applying device settings: %s', settings)
        self._lc_repo.set_settings(device_id, settings)
        #  Todo: if save last applied settings is on:
        #    save applied setting to user settings (device, device_id, channel, values)
        # todo: write success/failure to status bar

    @staticmethod
    def _convert_axis_to_profile(temps: List[int], duties: List[int]) -> List[Tuple[int, int]]:
        return list(zip(temps, duties))
