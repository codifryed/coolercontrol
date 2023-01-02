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
import time
from typing import Callable

from apscheduler.executors.pool import ThreadPoolExecutor
from apscheduler.job import Job
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.interval import IntervalTrigger

from coolercontrol.models.device import Device
from coolercontrol.repositories.daemon_repo import DaemonRepo
from coolercontrol.repositories.devices_repository import DevicesRepository
from coolercontrol.services.device_commander import DeviceCommander
from coolercontrol.services.dynamic_controls.lcd_controls import LcdControls
from coolercontrol.services.dynamic_controls.lighting_controls import LightingControls
from coolercontrol.services.notifications import Notifications
from coolercontrol.services.sleep_listener import SleepListener
from coolercontrol.view.uis.canvases.speed_control_canvas import SpeedControlCanvas
from coolercontrol.view_models.device_observer import DeviceObserver
from coolercontrol.view_models.device_subject import DeviceSubject
from coolercontrol.view_models.observer import Observer
from coolercontrol.view_models.subject import Subject

log = logging.getLogger(__name__)


class DevicesViewModel(DeviceSubject, Observer):
    """
    The View Model for interaction between the frontend and backend components.
    This class also acts similar to a double dispatcher,
    in which notifications pass in both directions,
        device state changes to the frontend
        and also UI state changes that need to be propagated to the devices
    _scheduler : The same background thread scheduler is used for all device communications, which helps
        keep any concurrent device communication interference to a minimum.
    """

    _scheduler: BackgroundScheduler = BackgroundScheduler(
        executors={'default': ThreadPoolExecutor(1)},
        job_defaults={'misfire_grace_time': 3, 'coalesce': False, 'replace_existing': False, 'max_instances': 20}
    )
    _device_repos: list[DevicesRepository] = []
    _device_commander: DeviceCommander = None
    _devices: list[Device] = []
    _observers: set[DeviceObserver] = set()
    _schedule_interval_seconds: int = 1
    _scheduled_events: list[Job] = []

    def __init__(self) -> None:
        super().__init__()
        self._notifications: Notifications = Notifications()
        self._sleep_listener: SleepListener = SleepListener(self._scheduled_events)
        self._scheduler.start()

    @property
    def devices(self) -> list[Device]:
        return self._devices

    def subscribe(self, observer: DeviceObserver) -> None:
        self._observers.add(observer)
        observer.notify_me(self)

    def unsubscribe(self, observer: DeviceObserver) -> None:
        self._observers.remove(observer)

    def notify_observers(self) -> None:
        for observer in self._observers:
            observer.notify_me(self)

    def init_devices_from_daemon(self) -> None:
        daemon_repo = DaemonRepo()
        self._device_repos.append(daemon_repo)
        self._devices.extend(daemon_repo.statuses)

    def init_scheduler_commander(self) -> None:
        daemon_repo = None
        for repo in self._device_repos:
            if isinstance(repo, DaemonRepo):
                daemon_repo = repo
        self._device_commander = DeviceCommander(
            daemon_repo, self._scheduler, self._notifications
        )
        # self._sleep_listener.set_speed_scheduler_jobs(self._speed_scheduler.scheduled_events)

    def schedule_status_updates(self) -> None:
        job: Job = self._scheduler.add_job(
            self._update_statuses,
            IntervalTrigger(seconds=self._schedule_interval_seconds),
            id='update_statuses'
        )
        self._scheduled_events.append(job)

    def shutdown_scheduler(self) -> None:
        for event in self._scheduled_events:
            event.remove()
        self._scheduled_events = []
        self._scheduler.shutdown()

    def shutdown(self) -> None:
        try:
            self._observers.clear()
            self._notifications.shutdown()
            self._sleep_listener.shutdown()
            self.shutdown_scheduler()
            for device_repo in self._device_repos:
                device_repo.shutdown()
        except BaseException as err:
            log.fatal('Unexpected shutdown exception', exc_info=err)

    def _update_statuses(self) -> None:
        for device_repo in self._device_repos:
            device_repo.update_statuses()
        self.notify_observers()

    def notify_me(self, subject: Subject) -> None:
        if self._device_commander is None:
            log.error('The LiquidctlRepo has not yet been initialized!!!')
            return
        if isinstance(subject, SpeedControlCanvas):
            self._device_commander.set_speed(subject)
        elif isinstance(subject, LightingControls):
            self._device_commander.set_lighting(subject)
        elif isinstance(subject, LcdControls):
            self._device_commander.set_lcd_screen(subject)
