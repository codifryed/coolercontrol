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

from models.settings import Settings, Setting
from models.speed_profile import SpeedProfile
from repositories.liquidctl_repo import LiquidctlRepo
from services.speed_scheduler import SpeedScheduler
from services.utils import MathUtils
from settings import Settings as SavedSettings
from view.uis.canvases.speed_control_canvas import SpeedControlCanvas

_LOG = logging.getLogger(__name__)


class DeviceCommander:
    _lc_repo: LiquidctlRepo
    _speed_scheduler: SpeedScheduler

    def __init__(self, lc_repo: LiquidctlRepo, speed_scheduler: SpeedScheduler) -> None:
        self._lc_repo = lc_repo
        self._speed_scheduler = speed_scheduler

    def set_speed(self, subject: SpeedControlCanvas) -> None:
        channel: str = subject.channel_name
        device_id: int = subject.device.lc_device_id
        if subject.current_speed_profile == SpeedProfile.FIXED:
            setting = Setting(speed_fixed=subject.fixed_duty)
            SavedSettings.save_fixed_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name, subject.fixed_duty
            )
            SavedSettings.save_applied_fixed_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name, subject.fixed_duty
            )
        elif subject.current_speed_profile == SpeedProfile.CUSTOM:
            setting = Setting(
                speed_profile=MathUtils.convert_axis_to_profile(subject.profile_temps, subject.profile_duties),
                profile_temp_source=subject.current_temp_source
            )
            SavedSettings.save_custom_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name,
                subject.profile_temps, subject.profile_duties
            )
            SavedSettings.save_applied_custom_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name,
                subject.profile_temps, subject.profile_duties
            )
        elif subject.current_speed_profile == SpeedProfile.NONE:
            SavedSettings.clear_applied_profile_for_channel(subject.device.name, device_id, channel)
            self._speed_scheduler.clear_channel_setting(subject.device, channel)
            return
        else:
            setting = Setting()
        settings = Settings({channel: setting})
        _LOG.info('Applying device settings: %s', settings)
        self._speed_scheduler.clear_channel_setting(subject.device, channel)
        if subject.current_speed_profile == SpeedProfile.CUSTOM \
                and (subject.device != subject.current_temp_source.device
                     and subject.current_temp_source.device.info.temp_ext_available) \
                or (subject.device == subject.current_temp_source.device
                    and subject.device.info.channels[channel].speed_options.manual_profiles_enabled):
            self._speed_scheduler.set_settings(subject.device, settings)
        else:
            self._lc_repo.set_settings(device_id, settings)
        # todo: write success/failure to status bar
