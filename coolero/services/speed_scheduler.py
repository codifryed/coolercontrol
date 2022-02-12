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
from typing import List, Dict

from apscheduler.job import Job
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.interval import IntervalTrigger

from models.device import Device, DeviceType
from models.settings import Settings, Setting
from models.status import Status
from repositories.liquidctl_repo import LiquidctlRepo
from services.utils import MathUtils
from view_models.device_observer import DeviceObserver
from view_models.device_subject import DeviceSubject

_LOG = logging.getLogger(__name__)
_APPLY_DUTY_THRESHOLD: int = 2


class SpeedScheduler(DeviceObserver):
    """
    This class enables the use of a scheduler to automatically set the speed on devices in relation to
    temperature sources that are not supported on the device itself.
    For ex. CPU based Fan and Pump controls or profile speed settings for devices that otherwise wouldn't support it.
    """

    _scheduler: BackgroundScheduler
    _scheduled_events: List[Job] = []
    _schedule_interval_seconds: int = 1
    _scheduled_settings: Dict[Device, Settings] = {}
    _lc_repo: LiquidctlRepo
    _devices: List[Device] = []
    _max_sample_size: int = 20

    def __init__(self, lc_repo: LiquidctlRepo, scheduler: BackgroundScheduler) -> None:
        self._lc_repo: LiquidctlRepo = lc_repo
        self._scheduler = scheduler
        self._start_speed_setting_schedule()
        self._duty_under_threshold_counter: int = 0

    def set_settings(self, device: Device, settings: Settings) -> None:
        if not settings.channel_settings:
            _LOG.error('Attempted to schedule speed profile without needed data: %s', settings)
        for channel, setting in settings.channel_settings.items():
            if setting.temp_source is None or not setting.speed_profile:
                _LOG.warning(
                    'There was an attempt to schedule a speed profile without the necessary info: %s', setting
                )
                break
            max_temp = setting.temp_source.device.info.temp_max
            normalized_profile = MathUtils.normalize_profile(
                setting.speed_profile, max_temp, device.info.channels[channel].speed_options.max_duty
            )
            normalized_setting = Setting(
                speed_profile=normalized_profile,
                temp_source=setting.temp_source
            )
            if device in self._scheduled_settings:
                self._scheduled_settings[device].channel_settings[channel] = normalized_setting
            else:
                self._scheduled_settings[device] = Settings({channel: normalized_setting})

    def clear_channel_setting(self, device: Device, channel: str) -> None:
        for set_device, settings in dict(self._scheduled_settings).items():
            for set_channel, _ in dict(settings.channel_settings).items():
                if set_channel == channel and set_device == device:
                    if len(settings.channel_settings) == 1:
                        del self._scheduled_settings[set_device]
                    else:
                        del settings.channel_settings[set_channel]

    def _update_speed(self) -> None:
        for device, settings in self._scheduled_settings.items():
            for channel, setting in settings.channel_settings.items():
                if setting.temp_source is None:
                    continue
                for temp in setting.temp_source.device.status.temps:
                    if setting.temp_source.name in [temp.frontend_name, temp.external_name]:
                        if setting.temp_source.device.type in [DeviceType.CPU, DeviceType.GPU]:
                            current_temp = self._get_smoothed_temperature(
                                setting.temp_source.device.status_history
                            )
                        else:
                            current_temp = temp.temp
                        break
                else:
                    continue
                duty_to_set: int = MathUtils.interpolate_profile(setting.speed_profile, current_temp)
                duty_above_threshold: bool = True
                if setting.last_manual_speeds_set:
                    difference_to_last_duty = abs(duty_to_set - setting.last_manual_speeds_set[-1])
                    threshold = _APPLY_DUTY_THRESHOLD if self._duty_under_threshold_counter < 4 else 0
                    duty_above_threshold = difference_to_last_duty > threshold
                if duty_above_threshold:
                    fixed_settings = Settings({channel: Setting(speed_fixed=duty_to_set,
                                                                temp_source=setting.temp_source)})
                    setting.last_manual_speeds_set.append(duty_to_set)
                    self._duty_under_threshold_counter = 0
                    if len(setting.last_manual_speeds_set) > self._max_sample_size:
                        setting.last_manual_speeds_set.pop(0)
                    _LOG.info('Applying device settings: %s', fixed_settings)
                    self._lc_repo.set_settings(device.lc_device_id, fixed_settings)
                else:
                    self._duty_under_threshold_counter += 1
                    _LOG.debug('Duty not above threshold to be applied to device. Skipping')
                    _LOG.debug('Last applied duties: %s', setting.last_manual_speeds_set)

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
        """CPU and GPU Temperatures can fluctuate quickly, this handles that with a moving average"""
        sample_size: int = min(SpeedScheduler._max_sample_size, len(status_history))
        #  currently, only single CPU and GPU measurements are supported
        device_temps = [status.temps[0].temp for status in status_history[-sample_size:]]
        return MathUtils.current_value_from_moving_average(device_temps, sample_size, exponential=True)
