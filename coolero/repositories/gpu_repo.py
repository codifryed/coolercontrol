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
from enum import Enum, auto
from typing import List, Tuple, Dict

import matplotlib
import numpy
import pyamdgpuinfo
from pyamdgpuinfo import GPUInfo

from coolero.models.device import Device, DeviceType
from coolero.models.device_info import DeviceInfo
from coolero.models.status import Status, TempStatus, ChannelStatus
from coolero.repositories.devices_repository import DevicesRepository
from coolero.services.shell_commander import ShellCommander
from coolero.settings import FeatureToggle

_LOG = logging.getLogger(__name__)
_DEFAULT_AMD_GPU_NAME: str = 'Radeon Graphics'
GPU_LOAD: str = 'GPU Load'
GPU_TEMP: str = 'GPU Temp'
GPU_FAN: str = 'GPU Fan'


class GpuType(Enum):
    NVIDIA = auto()
    AMD = auto()


class GpuRepo(DevicesRepository):
    """Repo for GPU Status"""

    _gpu_statuses: List[Device] = []
    _detected_gpu_types: Dict[GpuType, int] = defaultdict(lambda: 0)
    _has_multiple_gpus: bool = False

    def __init__(self) -> None:
        super().__init__()
        _LOG.info('Initialized with status: %s', self._gpu_statuses)

    @property
    def statuses(self) -> List[Device]:
        """Returns empty list if no GPU found"""
        return self._gpu_statuses

    def update_statuses(self) -> None:
        for index, (status, device_name) in enumerate(self._request_statuses()):
            self._gpu_statuses[index].status = status
            _LOG.debug('GPU device: %s status was updated with status: %s', device_name, status)

    def shutdown(self) -> None:
        self._gpu_statuses.clear()
        _LOG.debug("GPU Repo shutdown")

    def _initialize_devices(self) -> None:
        self._detect_gpu_types()
        colors = self._get_device_colors()
        for index, (status, device_name) in enumerate(self._request_statuses()):
            self._gpu_statuses.append(Device(
                _name=device_name,
                _type_id=(DeviceType.GPU, index + 1),
                _status_current=status,
                _colors={
                    GPU_TEMP: colors[index],
                    GPU_LOAD: colors[index],
                    GPU_FAN: colors[index]
                },
                _info=DeviceInfo(temp_max=100, temp_ext_available=True)
            ))

    def _detect_gpu_types(self) -> None:
        self._detected_gpu_types[GpuType.AMD] = pyamdgpuinfo.detect_gpus()
        len_adjustment = 2 if FeatureToggle.multi_gpu_testing else 0
        self._detected_gpu_types[GpuType.NVIDIA] = len(ShellCommander.get_nvidia_status()) + len_adjustment
        self._has_multiple_gpus = self._detected_gpu_types[GpuType.AMD] + self._detected_gpu_types[GpuType.NVIDIA] > 1
        if not self._detected_gpu_types[GpuType.AMD] and not self._detected_gpu_types[GpuType.NVIDIA]:
            _LOG.warning('No GPU Device detected')

    def _get_device_colors(self) -> List[str]:
        number_of_colors = self._detected_gpu_types[GpuType.AMD] + self._detected_gpu_types[GpuType.NVIDIA]
        colors_selectors = numpy.linspace(0, 1, number_of_colors)
        color_map = matplotlib.cm.get_cmap('Wistia')(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]

    def _request_statuses(self) -> List[Tuple[Status, str]]:
        statuses = []
        if self._detected_gpu_types[GpuType.AMD]:
            statuses.extend(self._request_amd_statuses())
        if self._detected_gpu_types[GpuType.NVIDIA]:
            statuses.extend(self._request_nvidia_statuses())
        return statuses

    def _request_amd_statuses(self) -> List[Tuple[Status, str]]:
        statuses = []
        for gpu_index in range(self._detected_gpu_types[GpuType.AMD]):
            gpu_amd: GPUInfo = pyamdgpuinfo.get_gpu(gpu_index)
            gpu_temp_name_prefix = f'#{gpu_index + 1} ' if self._has_multiple_gpus else ''
            gpu_name: str = gpu_amd.name if gpu_amd.name else _DEFAULT_AMD_GPU_NAME
            statuses.append(
                (
                    Status(
                        temps=[
                            TempStatus(GPU_TEMP, gpu_amd.query_temperature(), GPU_TEMP, gpu_temp_name_prefix + GPU_TEMP)
                        ],
                        channels=[ChannelStatus(GPU_LOAD, duty=gpu_amd.query_load())]
                    ),
                    gpu_name
                )
            )
        return statuses

    def _request_nvidia_statuses(self) -> List[Tuple[Status, str]]:
        statuses = []
        nvidia_statuses = ShellCommander.get_nvidia_status()
        if FeatureToggle.multi_gpu_testing:
            nvidia_statuses.extend(nvidia_statuses + nvidia_statuses)
        if len(nvidia_statuses) != self._detected_gpu_types[GpuType.NVIDIA]:
            _LOG.error(
                'Nvidia status update contained %d responses, but %d Nvidia gpus were detected',
                len(nvidia_statuses),
                self._detected_gpu_types[GpuType.NVIDIA]
            )
            return []
        starting_gpu_index = self._detected_gpu_types[GpuType.AMD] + 1 if self._has_multiple_gpus else 1
        for index, nvidia_status in enumerate(nvidia_statuses):
            temps = []
            channels = []
            test_temp_offset = index * 10 if FeatureToggle.multi_gpu_testing else 0
            if nvidia_status.temp is not None:
                gpu_temp_name_prefix = f'#{starting_gpu_index + index} ' if self._has_multiple_gpus else ''
                temps.append(
                    TempStatus(
                        GPU_TEMP,
                        nvidia_status.temp + test_temp_offset,
                        GPU_TEMP, gpu_temp_name_prefix + GPU_TEMP
                    )
                )
            if nvidia_status.load is not None:
                channels.append(ChannelStatus(
                    GPU_LOAD,
                    duty=nvidia_status.load + test_temp_offset
                ))
            if nvidia_status.fan_duty is not None:
                channels.append(ChannelStatus(
                    GPU_FAN,
                    duty=nvidia_status.fan_duty + test_temp_offset
                ))
            statuses.append(
                (
                    Status(temps=temps, channels=channels),
                    nvidia_status.name
                )
            )
        return statuses
