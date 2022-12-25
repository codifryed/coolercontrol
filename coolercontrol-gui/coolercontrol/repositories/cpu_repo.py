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
import platform
import subprocess
from typing import Optional, List, Dict

import psutil

from coolercontrol.models.device import Device, DeviceType
from coolercontrol.models.device_info import DeviceInfo
from coolercontrol.models.status import Status, TempStatus, ChannelStatus
from coolercontrol.repositories.devices_repository import DevicesRepository
from coolercontrol.settings import Settings

CPU_LOAD = 'CPU Load'
CPU_TEMP = 'CPU Temp'
_LOG = logging.getLogger(__name__)
# NOTE: Sensor and Label names are prioritized.
#  This is particularly helpful on devices like laptops where there can be multiple & somewhat different cpu readings
PSUTIL_CPU_SENSOR_NAMES: List[str] = ['thinkpad', 'k10temp', 'coretemp', 'zenpower']
_PSUTIL_CPU_STATUS_LABELS: List[str] = ['CPU', 'tctl', 'physical', 'package', 'tdie', '']


class CpuRepo(DevicesRepository):
    """Repo for CPU Status"""

    _cpu_statuses: List[Device] = []
    _current_sensor_name: str = ''
    _current_label_name: str = ''

    def __init__(self) -> None:
        super().__init__()
        _LOG.info('Successfully initialized')
        _LOG.debug('Initialized with status: %s', self._cpu_statuses)

    @property
    def statuses(self) -> List[Device]:
        return self._cpu_statuses

    def update_statuses(self) -> None:
        for cpu in self._cpu_statuses:
            cpu.status = self._request_status()
            _LOG.debug('CPU device: %s status was updated with status: %s',
                       cpu.name,
                       cpu.status)

    def shutdown(self) -> None:
        self._cpu_statuses.clear()
        _LOG.debug("CPU Repo shutdown")

    def _initialize_devices(self) -> None:
        status = self._request_status()
        cpu_name = self._get_cpu_name()
        if status:
            self._cpu_statuses.append(Device(
                _name=cpu_name,
                _type_id=(DeviceType.CPU, 1),
                _status_current=status,
                _colors={
                    CPU_TEMP: Settings.theme['app_color']['red'],
                    CPU_LOAD: Settings.theme['app_color']['red']
                },
                _info=DeviceInfo(temp_max=100, temp_ext_available=True)
            ))

    def _request_status(self) -> Optional[Status]:
        temp_sensors = psutil.sensors_temperatures()
        _LOG.debug('PSUTIL Temperatures detected: %s', temp_sensors)
        return self._request_quick_status(temp_sensors) \
            if self._current_sensor_name else self._request_new_status(temp_sensors)

    def _request_new_status(self, temp_sensors: Dict[str, List]) -> Optional[Status]:
        """This is used to find the correct sensors and labels for cpu data"""
        for sensor_name in PSUTIL_CPU_SENSOR_NAMES:
            if sensor_name in temp_sensors.keys():
                for label_sensor, current_temp, _, _ in temp_sensors[sensor_name]:
                    label = label_sensor.lower().replace(' ', '_')
                    for label_name in _PSUTIL_CPU_STATUS_LABELS:
                        if label_name in label:
                            self._current_sensor_name = sensor_name
                            self._current_label_name = label_sensor
                            cpu_usage = psutil.cpu_percent()
                            return Status(
                                temps=[TempStatus(CPU_TEMP, float(current_temp), CPU_TEMP, CPU_TEMP)],
                                channels=[ChannelStatus(CPU_LOAD, duty=int(cpu_usage))],
                            )
        _LOG.warning('No selected temperature found from psutil: %s', temp_sensors)
        return None

    def _request_quick_status(self, temp_sensors: Dict[str, List]) -> Optional[Status]:
        """This method is called once we know the correct and existing sensors names and labels.
        It is a bit more efficient for regular status updates"""
        for label_sensor, current_temp, _, _ in temp_sensors[self._current_sensor_name]:
            if self._current_label_name == label_sensor:
                cpu_usage = psutil.cpu_percent()
                return Status(
                    temps=[TempStatus(CPU_TEMP, float(current_temp), CPU_TEMP, CPU_TEMP)],
                    channels=[ChannelStatus(CPU_LOAD, duty=int(cpu_usage))],
                )
        _LOG.error("Known CPU sensor not found. This shouldn't happen.")
        return self._request_new_status(temp_sensors)

    @staticmethod
    def _get_cpu_name() -> str:
        if platform.system() == 'Linux':
            try:
                for line in (subprocess.check_output('LC_ALL=C lscpu', shell=True).strip()).decode().splitlines():
                    if 'model name' in line.lower():
                        return line.split(':')[1].strip()
                _LOG.warning('CPU Model Name not found.')
            except BaseException as ex:
                _LOG.warning('Unable to call lscpu from the shell. %s', ex)
        return 'cpu'
