#  Coolero - monitor and control your cooling and other devices
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
from statistics import mean
from typing import List, Tuple, Set

import matplotlib
import numpy

from coolero.models.device import Device, DeviceType
from coolero.models.device_info import DeviceInfo
from coolero.models.status import Status, TempStatus
from coolero.repositories.cpu_repo import CPU_TEMP
from coolero.repositories.devices_repository import DevicesRepository
from coolero.repositories.gpu_repo import GPU_TEMP

_LOG = logging.getLogger(__name__)
_AVG_ALL: str = 'Average All'
_LIQUID_TEMP_NAMES: Tuple[str, str] = ('Liquid', 'Water')


class CompositeRepo(DevicesRepository):
    """This class if for combining different temp sources from devices for lots of possibilities"""
    _composite_device_status: Device | None

    def __init__(self, devices: List[Device]) -> None:
        self._devices = devices
        super().__init__()
        _LOG.info('Initialized with status: %s', self._composite_device_status)

    def _initialize_devices(self) -> None:
        if len(self._devices) < 2:
            return
        if all_temps := self._collect_all_temps():
            avg_all_temps = self._get_avg_all_temps(all_temps)
            delta_cpu_temps = self._get_delta_cpu_liquid_temps(all_temps)
            delta_gpu_temps = self._get_delta_gpu_liquid_temps(all_temps)
            composite_temps = avg_all_temps + delta_cpu_temps + delta_gpu_temps
            colors = self._get_device_colors(len(composite_temps))
            self._composite_device_status = Device(
                _name='Composite',
                _type_id=(DeviceType.COMPOSITE, 1),
                _status_current=Status(temps=composite_temps),
                _colors={
                    temp_status.name: colors[index]
                    for index, temp_status in enumerate(composite_temps)
                },
                _info=DeviceInfo(
                    temp_min=0,
                    temp_max=100,
                    temp_ext_available=True,
                    profile_max_length=21
                )
            )

    @property
    def statuses(self) -> List[Device]:
        return [self._composite_device_status]

    def update_statuses(self) -> None:
        if self._composite_device_status is None:
            return
        if all_temps := self._collect_all_temps():
            avg_all_temps = self._get_avg_all_temps(all_temps)
            delta_cpu_temps = self._get_delta_cpu_liquid_temps(all_temps)
            delta_gpu_temps = self._get_delta_gpu_liquid_temps(all_temps)
            composite_temps = avg_all_temps + delta_cpu_temps + delta_gpu_temps
            self._composite_device_status.status = Status(temps=composite_temps)
            _LOG.debug('Composite device: %s status was updated with status: %s',
                       self._composite_device_status.name,
                       self._composite_device_status.status)

    def shutdown(self) -> None:
        self._composite_device_status = None
        _LOG.debug("Composite Repo shutdown")

    def _collect_all_temps(self) -> Set[Tuple[str, float, int]]:
        all_temps = {
            (temp_status.external_name, temp_status.temp, device.type_id)
            for device in self._devices
            if device.type != DeviceType.COMPOSITE
            for temp_status in device.status.temps
        }
        return all_temps if len(all_temps) > 1 else set()

    @staticmethod
    def _get_avg_all_temps(all_temps: Set[Tuple[str, float, int]]) -> List[TempStatus]:
        _, temps, _ = zip(*all_temps)
        return [TempStatus(_AVG_ALL, mean(temps), _AVG_ALL, _AVG_ALL)]

    @staticmethod
    def _get_delta_cpu_liquid_temps(all_temps: Set[Tuple[str, float, int]]) -> List[TempStatus]:
        deltas: List[TempStatus] = []
        if cpu_temp := next((temp for name, temp, _ in all_temps if name == CPU_TEMP), None):
            liquid_temps = filter(
                lambda name_temp: _LIQUID_TEMP_NAMES[0] in name_temp[0] or _LIQUID_TEMP_NAMES[1] in name_temp[0],
                all_temps
            )
            for name, temp, _ in liquid_temps:
                delta_temp_name = f'Δ CPU {name}'
                deltas.append(
                    TempStatus(delta_temp_name, abs(cpu_temp - temp), delta_temp_name, delta_temp_name)
                )
        return deltas

    @staticmethod
    def _get_delta_gpu_liquid_temps(all_temps: Set[Tuple[str, float, int]]) -> List[TempStatus]:
        deltas: List[TempStatus] = []
        if gpu_temps := [(temp, dev_id) for name, temp, dev_id in all_temps if name == GPU_TEMP]:
            liquid_temps = filter(
                lambda name_temp_id: _LIQUID_TEMP_NAMES[0] in name_temp_id[0]
                                     or _LIQUID_TEMP_NAMES[1] in name_temp_id[0],
                all_temps
            )
            number_gpus = len(gpu_temps)
            for name, temp, device_id in liquid_temps:
                for gpu_temp, gpu_id in gpu_temps:
                    delta_temp_name = f'Δ GPU {name}' if number_gpus == 1 else f'Δ GPU#{gpu_id} {name}'
                    deltas.append(
                        TempStatus(delta_temp_name, abs(gpu_temp - temp), delta_temp_name, delta_temp_name)
                    )
        return deltas

    @staticmethod
    def _get_device_colors(number_of_colors: int) -> List[str]:
        colors_selectors = numpy.linspace(0.5, 0.9, number_of_colors)
        color_map = matplotlib.cm.get_cmap('copper')(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]
