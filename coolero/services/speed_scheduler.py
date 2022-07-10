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
from collections import defaultdict
from typing import List, Dict, Optional

from apscheduler.job import Job
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.interval import IntervalTrigger

from coolero.models.device import Device, DeviceType
from coolero.models.settings import Setting
from coolero.models.status import Status
from coolero.repositories.hwmon_repo import HwmonRepo
from coolero.repositories.liquidctl_repo import LiquidctlRepo
from coolero.services.utils import MathUtils
from coolero.settings import Settings, UserSettings
from coolero.view_models.device_observer import DeviceObserver
from coolero.view_models.device_subject import DeviceSubject

_LOG = logging.getLogger(__name__)
_APPLY_DUTY_THRESHOLD: int = 2
_MAX_UNDER_THRESHOLD_COUNTER: int = 5


class SpeedScheduler(DeviceObserver):
    """
    This class enables the use of a scheduler to automatically set the speed on devices in relation to
    temperature sources that are not supported on the device itself.
    For ex. CPU based Fan and Pump controls or profile speed settings for devices that otherwise wouldn't support it.
    """

    _scheduler: BackgroundScheduler
    _scheduled_events: List[Job] = []
    _schedule_interval_seconds: int = 1
    _scheduled_settings: Dict[Device, List[Setting]] = defaultdict(list)
    _lc_repo: LiquidctlRepo
    _devices: List[Device] = []
    _max_sample_size: int = 20

    def __init__(self, lc_repo: LiquidctlRepo, hwmon_repo: HwmonRepo, scheduler: BackgroundScheduler) -> None:
        self._lc_repo: LiquidctlRepo = lc_repo
        self._hwmon_repo: HwmonRepo = hwmon_repo
        self._scheduler = scheduler
        self._handle_dynamic_temps: bool = Settings.user.value(
            UserSettings.ENABLE_DYNAMIC_TEMP_HANDLING, defaultValue=True, type=bool)
        self._start_speed_setting_schedule()

    def set_settings(self, device: Device, setting: Setting) -> Optional[str]:
        if setting.temp_source is None or not setting.speed_profile:
            _LOG.warning(
                'There was an attempt to schedule a speed profile without the necessary info: %s', setting
            )
            return None
        if device.type == DeviceType.HWMON and not self._hwmon_repo.daemon_is_running():
            return 'ERROR coolerod not running'
        max_temp = setting.temp_source.device.info.temp_max
        normalized_profile = MathUtils.norm_profile(
            setting.speed_profile, max_temp, device.info.channels[setting.channel_name].speed_options.max_duty
        )
        normalized_setting = Setting(
            setting.channel_name,
            speed_profile=normalized_profile,
            temp_source=setting.temp_source
        )
        for index, scheduled_setting in enumerate(self._scheduled_settings[device]):
            if scheduled_setting.channel_name == setting.channel_name:
                self._scheduled_settings[device][index] = normalized_setting
                break
        else:
            self._scheduled_settings[device].append(normalized_setting)
        return device.name

    def clear_channel_setting(self, device: Device, channel: str) -> bool:
        device_settings = self._scheduled_settings.get(device)
        if device_settings is None:
            return False
        for index, setting in enumerate(device_settings):
            if setting.channel_name == channel:
                if len(device_settings) <= 1:
                    del self._scheduled_settings[device]
                else:
                    self._scheduled_settings[device].pop(index)
                return True
        return False

    def _update_speed(self) -> None:
        for device, settings in self._scheduled_settings.items():
            for setting in settings:
                if setting.temp_source is None:
                    continue
                current_temp = self._get_current_temp(setting)
                if current_temp is None:
                    continue
                duty_to_set: int = MathUtils.interpol_profile(setting.speed_profile, current_temp)
                if self._duty_is_above_threshold(setting, duty_to_set):
                    self._set_speed(device, setting, duty_to_set)
                else:
                    setting.under_threshold_counter += 1
                    _LOG.debug('Duty not above threshold to be applied to device. Skipping')
                    _LOG.debug('Last applied duties: %s', setting.last_manual_speeds_set)

    def _get_current_temp(self, setting: Setting) -> float | None:
        return next(
            (
                self._get_smoothed_temperature(setting.temp_source.device.status_history)  # only cpu & gpu
                if self._handle_dynamic_temps and setting.temp_source.device.type in [DeviceType.CPU, DeviceType.GPU]
                else temp.temp
                for temp in setting.temp_source.device.status.temps
                # temp_source.name is set to either frontend_name or external_name:
                if setting.temp_source.name in [temp.frontend_name, temp.external_name]
            ),
            None
        )

    @staticmethod
    def _duty_is_above_threshold(setting: Setting, duty_to_set: int) -> bool:
        if not setting.last_manual_speeds_set:
            return True
        difference_to_last_duty = abs(duty_to_set - setting.last_manual_speeds_set[-1])
        threshold = _APPLY_DUTY_THRESHOLD if setting.under_threshold_counter < _MAX_UNDER_THRESHOLD_COUNTER else 0
        return difference_to_last_duty > threshold

    def _set_speed(self, device, setting, duty_to_set):
        fixed_setting = Setting(
            setting.channel_name, speed_fixed=duty_to_set, temp_source=setting.temp_source, pwm_mode=setting.pwm_mode
        )
        setting.last_manual_speeds_set.append(duty_to_set)
        setting.under_threshold_counter = 0
        if len(setting.last_manual_speeds_set) > self._max_sample_size:
            setting.last_manual_speeds_set.pop(0)
        _LOG.info('Applying scheduled settings for %s', device.name)
        _LOG.debug('Applying device settings: %s', fixed_setting)
        if device.type == DeviceType.LIQUIDCTL:
            self._lc_repo.set_settings(device.type_id, fixed_setting)
        elif device.type == DeviceType.HWMON:
            if self._hwmon_repo.set_settings(device.type_id, fixed_setting):
                _LOG.debug('Successfully applied hwmon setting from speed scheduler')
            else:
                _LOG.error('Unsuccessfully applied hwmon setting from speed scheduler!')

    def notify_me(self, subject: DeviceSubject) -> None:
        if not self._devices:
            self._devices = subject.devices

    def shutdown(self) -> None:
        for event in self._scheduled_events:
            event.remove()
        self._scheduled_events = []

    def _start_speed_setting_schedule(self) -> None:
        job: Job = self._scheduler.add_job(
            self._update_speed,
            IntervalTrigger(seconds=self._schedule_interval_seconds),
            id='update_speeds'
        )
        self._scheduled_events.append(job)

    @staticmethod
    def _get_smoothed_temperature(status_history: List[Status]) -> float:
        """
        CPU and GPU Temperatures can fluctuate quickly, this handles that with a moving average.
        Only CPU and GPU measurements are currently supported by this function. (single temps)
        """
        sample_size: int = min(SpeedScheduler._max_sample_size, len(status_history))
        device_temps = [status.temps[0].temp for status in status_history[-sample_size:]]
        return MathUtils.current_value_from_moving_average(device_temps, sample_size, exponential=True)
