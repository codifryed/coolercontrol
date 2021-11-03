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
from time import sleep
from typing import List, Set

from apscheduler.job import Job
from apscheduler.schedulers.background import BackgroundScheduler

from models.device import Device
from repositories.cpu_repo import CpuRepo
from repositories.devices_repository import DevicesRepository
from repositories.gpu_repo import GpuRepo
from repositories.liquidctl_repo import LiquidctlRepo
from services.device_commander import DeviceCommander
from view.uis.canvases.speed_control_canvas import SpeedControlCanvas
from view_models.device_observer import DeviceObserver
from view_models.device_subject import DeviceSubject
from view_models.observer import Observer
from view_models.subject import Subject

_LOG = logging.getLogger(__name__)


class DevicesViewModel(DeviceSubject, Observer):
    """
    The View Model for interaction between the frontend and backend components.
    This class also acts similar to a double dispatcher,
    in which notifications pass in both directions,
        device state changes to the frontend
        and also UI state changes that need to be propagated to the devices
    """

    _scheduler: BackgroundScheduler = BackgroundScheduler()
    _device_repos: List[DevicesRepository] = []
    _device_commander: DeviceCommander
    _device_statuses: List[Device] = []
    _observers: Set[DeviceObserver] = set()
    _schedule_interval_seconds: int = 1
    _scheduled_events: List[Job] = []

    def __init__(self) -> None:
        super().__init__()
        self._log = logging.getLogger(__name__)
        self._scheduler.start()

    @property
    def device_statuses(self) -> List[Device]:
        return self._device_statuses

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
        # todo: rename everywhere status to devices (confusing)
        self._device_statuses.extend(cpu_repo.statuses)

    def init_gpu_repo(self) -> None:
        gpu_repo = GpuRepo()
        self._device_repos.append(gpu_repo)
        self._device_statuses.extend(gpu_repo.statuses)

    def init_liquidctl_repo(self) -> None:
        liquidctl_repo = LiquidctlRepo()
        self._device_repos.append(liquidctl_repo)
        self._device_commander = DeviceCommander(liquidctl_repo)
        self._device_statuses.extend(liquidctl_repo.statuses)

    def schedule_status_updates(self) -> None:
        job: Job = self._scheduler.add_job(
            self._update_statuses,
            'interval',
            seconds=self._schedule_interval_seconds,
            id='update_statuses'
        )
        self._scheduled_events.append(job)

    def unschedule_status_updates(self) -> None:
        for event in self._scheduled_events:
            event.remove()
        self._scheduled_events = list()

    def shutdown(self) -> None:
        self._observers.clear()
        self.unschedule_status_updates()
        sleep(0.5)  # need to wait until all jobs are done
        for device_repo in self._device_repos:
            device_repo.shutdown()

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
