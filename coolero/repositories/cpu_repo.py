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
import platform
import subprocess
from typing import Optional, List

import psutil

from models.channel_info import ChannelInfo
from models.device import Device, DeviceType
from models.device_info import DeviceInfo
from models.speed_options import SpeedOptions
from models.status import Status
from repositories.devices_repository import DevicesRepository

_LOG = logging.getLogger(__name__)


class CpuRepo(DevicesRepository):
    """Repo for CPU Status"""

    _cpu_statuses: List[Device] = []

    def __init__(self) -> None:
        super().__init__()
        _LOG.info('Initialized with status: %s', self._cpu_statuses)

    @property
    def statuses(self) -> List[Device]:
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
        cpu_name = self._get_cpu_name()
        channel_info = ChannelInfo(SpeedOptions(
            # todo: build algorithm and scheduler for cpu fan/pump speed profile
            profiles_enabled=False,
            fixed_enabled=True
        ))
        if status:
            self._cpu_statuses.append(Device(
                # todo: adjust to handle multiple cpus (make device_id general)
                cpu_name,
                DeviceType.CPU,
                status,
                _device_info=DeviceInfo(channels={'pump': channel_info, 'fan': channel_info})
            ))

    @staticmethod
    def _request_status() -> Optional[Status]:
        temp_sensors = psutil.sensors_temperatures().items()
        _LOG.debug('PSUTIL Temperatures detected: %s', temp_sensors)
        for name, list_items in temp_sensors:
            if name in ['k10temp', 'coretemp']:
                for label_sensor, current_temp, _, _ in list_items:
                    label = label_sensor.lower().replace(' ', '_')
                    cpu_usage = psutil.cpu_percent()
                    # AMD uses tctl for cpu temp for fan control (not die temp)
                    if 'tctl' in label or 'physical' in label or 'package' in label:
                        return Status(device_temperature=float(current_temp), load_percent=cpu_usage)
        _LOG.warning('No selected temperature found from psutil: %s', temp_sensors)
        return None

    @staticmethod
    def _get_cpu_name() -> str:
        if platform.system() == 'Linux':
            for line in (subprocess.check_output('lscpu', shell=True).strip()).decode().splitlines():
                if 'model name' in line.lower():
                    return line.split(':')[1].strip()
        return 'cpu'
