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
from typing import Callable

from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.date import DateTrigger

from coolercontrol.models.device import DeviceType, Device
from coolercontrol.models.lcd_mode import LcdModeType
from coolercontrol.models.lighting_mode import LightingModeType
from coolercontrol.models.settings import Setting
from coolercontrol.models.speed_profile import SpeedProfile
from coolercontrol.repositories.daemon_repo import DaemonRepo
from coolercontrol.services.dynamic_controls.lcd_controls import LcdControls
from coolercontrol.services.dynamic_controls.lighting_controls import LightingControls
from coolercontrol.services.notifications import Notifications
from coolercontrol.services.utils import MathUtils
from coolercontrol.settings import Settings as SavedSettings
from coolercontrol.view.uis.canvases.speed_control_canvas import SpeedControlCanvas

log = logging.getLogger(__name__)


class DeviceCommander:

    def __init__(self,
                 daemon_repo: DaemonRepo,
                 base_scheduler: BackgroundScheduler,
                 notifications: Notifications) -> None:
        self._daemon_repo = daemon_repo
        self._base_scheduler = base_scheduler
        self._notifications: Notifications = notifications

    def set_speed(self, subject: SpeedControlCanvas) -> None:
        channel: str = subject.channel_name
        device_id: int = subject.device.type_id
        device_uid: str = subject.device.uid
        if subject.current_speed_profile == SpeedProfile.FIXED:
            setting = Setting(channel, speed_fixed=subject.fixed_duty, pwm_mode=subject.pwm_mode)
            SavedSettings.save_fixed_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name, subject.fixed_duty,
                subject.pwm_mode
            )
            SavedSettings.save_applied_fixed_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name, subject.fixed_duty,
                subject.pwm_mode
            )
        elif subject.current_speed_profile == SpeedProfile.CUSTOM:
            setting = Setting(
                channel,
                speed_profile=MathUtils.convert_axis_to_profile(subject.profile_temps, subject.profile_duties),
                temp_source=subject.current_temp_source,
                pwm_mode=subject.pwm_mode
            )
            SavedSettings.save_custom_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name,
                subject.profile_temps, subject.profile_duties, subject.pwm_mode
            )
            SavedSettings.save_applied_custom_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name,
                subject.profile_temps, subject.profile_duties, subject.pwm_mode
            )
        elif subject.current_speed_profile in [SpeedProfile.NONE, SpeedProfile.DEFAULT]:
            # clearing / setting to default
            SavedSettings.save_applied_none_default_profile(
                subject.device.name, device_id, channel, subject.current_temp_source.name,
                subject.current_speed_profile, subject.pwm_mode
            )
            setting = Setting(channel, pwm_mode=subject.pwm_mode, reset_to_default=True)
            log.info('Clearing settings / setting to default for %s', subject.device.name)
            self._add_to_device_jobs(
                lambda: self._notifications.settings_applied(
                    self._daemon_repo.set_settings(device_uid, setting)
                )
            )
            return  # nothing more to do in this case
        else:
            log.error("Situation is not applicable to set device speed settings")
            return
        log.info('Setting settings for %s', subject.device.name)
        log.debug('Setting speed device settings: %s', setting)
        self._add_to_device_jobs(
            lambda: self._notifications.settings_applied(
                self._daemon_repo.set_settings(device_uid, setting)
            )
        )

    def set_lighting(self, subject: LightingControls) -> None:
        if subject.current_set_settings is None:
            return
        device_id, lighting_setting = subject.current_set_settings
        if lighting_setting.lighting_mode.type != LightingModeType.LC:
            return  # only LC lighting modes are currently supported
        associated_device: Device | None = next(
            (
                device for device in self._daemon_repo.devices.values()
                if device.type == DeviceType.LIQUIDCTL and device.type_id == device_id
            ),
            None,
        )
        if associated_device is None:
            log.error("Device not found for setting lighting settings: Liquidctl device #%s", device_id)
            return
        SavedSettings.save_lighting_settings()
        log.info('Setting lighting settings for Liquidctl device #%s', device_id)
        log.debug('Setting lighting device settings: %s', lighting_setting)
        self._add_to_device_jobs(
            lambda: self._notifications.settings_applied(
                self._daemon_repo.set_settings(associated_device.uid, lighting_setting)
            )
        )

    def set_lcd_screen(self, subject: LcdControls) -> None:
        if subject.current_set_settings is None:
            return
        device_id, lcd_setting = subject.current_set_settings
        associated_device: Device | None = next(
            (
                device for device in self._daemon_repo.devices.values()
                if device.type == DeviceType.LIQUIDCTL and device.type_id == device_id
            ),
            None,
        )
        if associated_device is None:
            log.error("Device not found for setting LCD settings: Liquidctl device #%s", device_id)
            return
        SavedSettings.save_lcd_settings()
        if lcd_setting.lcd_mode.type == LcdModeType.NONE or (
                lcd_setting.lcd.mode == "image" and lcd_setting.lcd.image_file_processed is None):
            return  # do nothing
        log.info('Setting LCD settings for Liquidctl device #%s', device_id)
        log.debug('Setting LCD device settings: %s', lcd_setting)
        self._add_to_device_jobs(
            lambda: self._notifications.settings_applied(
                self._daemon_repo.set_settings(associated_device.uid, lcd_setting)
            )
        )

    def _add_to_device_jobs(self, set_function: Callable) -> None:
        self._base_scheduler.add_job(
            set_function,
            DateTrigger(),  # defaults to now()
        )
