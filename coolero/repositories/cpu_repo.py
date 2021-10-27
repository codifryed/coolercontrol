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
from typing import Optional, List

import psutil

from models.channel_info import ChannelInfo
from models.device_info import DeviceInfo
from models.device_status import DeviceStatus
from models.speed_options import SpeedOptions
from models.status import Status
from repositories.devices_repository import DevicesRepository

_LOG = logging.getLogger(__name__)


class CpuRepo(DevicesRepository):
    """Repo for CPU Status"""

    _cpu_statuses: List[DeviceStatus] = []

    def __init__(self) -> None:
        super().__init__()
        _LOG.info('Initialized with status: %s', self._cpu_statuses)

    @property
    def statuses(self) -> List[DeviceStatus]:
        return self._cpu_statuses

    def update_statuses(self) -> None:
        for cpu in self._cpu_statuses:
            cpu.status = self._request_status()
            _LOG.debug('CPU device: %s status was updated with status: %s',
                       cpu.device_name,
                       cpu.status)

    def shutdown(self) -> None:
        self._cpu_statuses.clear()
        _LOG.debug("CPU Repo shutdown")

    def _initialize_devices(self) -> None:
        status = self._request_status()
        # todo: use feature toggles before release: (set this or not depending on toggle)
        channel_info = ChannelInfo(SpeedOptions(
            # todo: build scripts for these options:
            profiles_enabled=False,
            fixed_enabled=True
        ))
        if status:
            self._cpu_statuses.append(DeviceStatus(
                # todo: adjust to handle multiple gpus (make device_id general)
                'cpu',
                status,
                _device_info=DeviceInfo(channels={'pump': channel_info, 'fan': channel_info})
            ))

    @staticmethod
    def _request_status() -> Optional[Status]:
        for _, list_items in psutil.sensors_temperatures().items():
            for label, current, _, _ in list_items:
                sensor_name = label.lower().replace(' ', '_')
                # todo: INTEL???
                # AMD uses tctl for cpu temp for fan control (not die temp for ex.)
                if 'tctl' in sensor_name:  # AMD or Intel
                    cpu_temp: float = current
                    cpu_usage = psutil.cpu_percent()
                    return Status(device_temperature=cpu_temp, load_percent=cpu_usage)
        return None
