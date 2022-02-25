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
from typing import List, Set

from apscheduler.executors.pool import ThreadPoolExecutor
from apscheduler.job import Job
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.interval import IntervalTrigger

from coolero.models.device import Device
from coolero.repositories.composite_repo import CompositeRepo
from coolero.repositories.cpu_repo import CpuRepo
from coolero.repositories.devices_repository import DevicesRepository
from coolero.repositories.gpu_repo import GpuRepo
from coolero.repositories.liquidctl_repo import LiquidctlRepo
from coolero.services.device_commander import DeviceCommander
from coolero.services.dynamic_controls.lighting_controls import LightingControls
from coolero.services.notifications import Notifications
from coolero.services.speed_scheduler import SpeedScheduler
from coolero.view.uis.canvases.speed_control_canvas import SpeedControlCanvas
from coolero.view_models.device_observer import DeviceObserver
from coolero.view_models.device_subject import DeviceSubject
from coolero.view_models.observer import Observer
from coolero.view_models.subject import Subject

_LOG = logging.getLogger(__name__)


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
        job_defaults={'misfire_grace_time': 3, 'coalesce': False, 'replace_existing': False, 'max_instances': 10}
    )
    _device_repos: List[DevicesRepository] = []
    _device_commander: DeviceCommander = None
    _speed_scheduler: SpeedScheduler = None
    _devices: List[Device] = []
    _observers: Set[DeviceObserver] = set()
    _schedule_interval_seconds: int = 1
    _scheduled_events: List[Job] = []
    _notifications: Notifications = Notifications()

    def __init__(self) -> None:
        super().__init__()
        self._scheduler.start()

    @property
    def devices(self) -> List[Device]:
        return self._devices

    def subscribe(self, observer: DeviceObserver) -> None:
        self._observers.add(observer)
        observer.notify_me(self)

    def unsubscribe(self, observer: DeviceObserver) -> None:
        self._observers.remove(observer)

    def notify_observers(self) -> None:
        for observer in self._observers:
            observer.notify_me(self)

    def init_cpu_repo(self) -> None:
        cpu_repo = CpuRepo()
        self._device_repos.append(cpu_repo)
        self._devices.extend(cpu_repo.statuses)

    def init_gpu_repo(self) -> None:
        gpu_repo = GpuRepo()
        self._device_repos.append(gpu_repo)
        self._devices.extend(gpu_repo.statuses)

    def init_liquidctl_repo(self) -> None:
        liquidctl_repo = LiquidctlRepo()
        self._device_repos.append(liquidctl_repo)
        self._speed_scheduler = SpeedScheduler(liquidctl_repo, self._scheduler)
        self._device_commander = DeviceCommander(
            liquidctl_repo, self._scheduler, self._speed_scheduler, self._notifications
        )
        self.subscribe(self._speed_scheduler)
        self._devices.extend(liquidctl_repo.statuses)

    def init_composite_repo(self) -> None:
        """needs to be initialized last"""
        composite_repo = CompositeRepo(self._devices)
        self._device_repos.append(composite_repo)
        self._devices.extend(composite_repo.statuses)

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
            if self._speed_scheduler is not None:
                self._speed_scheduler.shutdown()
            self.shutdown_scheduler()
            for device_repo in self._device_repos:
                device_repo.shutdown()
        except BaseException as err:
            _LOG.fatal('Unexpected shutdown exception', exc_info=err)

    def _update_statuses(self) -> None:
        for device_repo in self._device_repos:
            device_repo.update_statuses()
        self.notify_observers()

    def notify_me(self, subject: Subject) -> None:
        if self._device_commander is None:
            _LOG.error('The LiquidctlRepo has not yet been initialized!!!')
            return
        if isinstance(subject, SpeedControlCanvas):
            self._device_commander.set_speed(subject)
        elif isinstance(subject, LightingControls):
            self._device_commander.set_lighting(subject)
