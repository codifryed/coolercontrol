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
from enum import Enum, auto
from typing import Optional, List, Tuple

import pyamdgpuinfo
from pyamdgpuinfo import GPUInfo

from coolero.models.device import Device, DeviceType
from coolero.models.device_info import DeviceInfo
from coolero.models.status import Status, TempStatus, ChannelStatus
from coolero.models.status_nvidia import StatusNvidia
from coolero.repositories.devices_repository import DevicesRepository
from coolero.services.shell_commander import ShellCommander
from coolero.settings import Settings

GPU_LOAD: str = 'GPU Load'
GPU_TEMP: str = 'GPU Temp'
GPU_FAN: str = 'GPU Fan'
_LOG = logging.getLogger(__name__)


class DetectedGPUType(Enum):
    NVIDIA = auto()
    AMD = auto()


class GpuRepo(DevicesRepository):
    """Repo for GPU Status"""

    _gpu_statuses: List[Device] = []
    _detected_gpu_type: Optional[DetectedGPUType] = None

    def __init__(self) -> None:
        super().__init__()
        _LOG.info('Initialized with status: %s', self._gpu_statuses)

    @property
    def statuses(self) -> List[Device]:
        """Returns empty list if no GPU found"""
        return self._gpu_statuses

    def update_statuses(self) -> None:
        for gpu in self._gpu_statuses:
            gpu.status, _ = self._request_status()
            _LOG.debug('GPU device: %s status was updated with status: %s',
                       gpu.name,
                       gpu.status)

    def shutdown(self) -> None:
        self._gpu_statuses.clear()
        _LOG.debug("GPU Repo shutdown")

    def _initialize_devices(self) -> None:
        self._detect_gpu_type()
        status, device_name = self._request_status()
        if status is not None:
            self._gpu_statuses.append(Device(
                # todo: adjust to handle multiple gpus (make device_id general)
                _name=device_name,
                _type=DeviceType.GPU,
                _status_current=status,
                _colors={
                    GPU_TEMP: Settings.theme['app_color']['yellow'],
                    GPU_LOAD: Settings.theme['app_color']['yellow'],
                    GPU_FAN: Settings.theme['app_color']['yellow']
                },
                _info=DeviceInfo(temp_max=100, temp_ext_available=True)
            ))

    def _detect_gpu_type(self) -> None:
        if len(ShellCommander.get_nvidia_status()) > 0:
            self._detected_gpu_type = DetectedGPUType.NVIDIA
        elif pyamdgpuinfo.detect_gpus() > 0:
            self._detected_gpu_type = DetectedGPUType.AMD
        else:
            _LOG.warning('No GPU Device detected')

    def _request_status(self) -> Tuple[Optional[Status], Optional[str]]:
        if self._detected_gpu_type == DetectedGPUType.NVIDIA:
            gpu_nvidia: StatusNvidia = ShellCommander.get_nvidia_status()[0]
            temps = []
            channels = []
            if gpu_nvidia.temp is not None:
                temps.append(TempStatus(GPU_TEMP, gpu_nvidia.temp, GPU_TEMP, GPU_TEMP))
            if gpu_nvidia.load is not None:
                channels.append(ChannelStatus(GPU_LOAD, duty=gpu_nvidia.load))
            if gpu_nvidia.fan_duty is not None:
                channels.append(ChannelStatus(GPU_FAN, duty=gpu_nvidia.fan_duty))
            return Status(
                temps=temps,
                channels=channels
            ), gpu_nvidia.name
        if self._detected_gpu_type == DetectedGPUType.AMD:
            gpu_amd: GPUInfo = pyamdgpuinfo.get_gpu(0)
            return Status(
                temps=[TempStatus(GPU_TEMP, gpu_amd.query_temperature(), GPU_TEMP, GPU_TEMP)],
                channels=[ChannelStatus(GPU_LOAD, duty=gpu_amd.query_load())]
            ), gpu_amd.name
        return None, None
