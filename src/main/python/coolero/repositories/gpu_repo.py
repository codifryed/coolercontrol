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

import GPUtil
import pyamdgpuinfo

from models.channel_info import ChannelInfo
from models.device_info import DeviceInfo
from models.device_status import DeviceStatus
from models.speed_options import SpeedOptions
from models.status import Status
from repositories.devices_repository import DevicesRepository

_LOG = logging.getLogger(__name__)


class GpuRepo(DevicesRepository):
    """Repo for GPU Status"""

    _gpu_statuses: List[DeviceStatus] = []

    def __init__(self) -> None:
        super().__init__()
        _LOG.info('Initialized with status: %s', self._gpu_statuses)

    @property
    def statuses(self) -> List[DeviceStatus]:
        """Returns empty list if no GPU found"""
        return self._gpu_statuses

    def update_statuses(self) -> None:
        for gpu in self._gpu_statuses:
            gpu.status = self._request_status()
            _LOG.debug('GPU device: %s status was updated with status: %s',
                       gpu.device_name,
                       gpu.status)

    def shutdown(self) -> None:
        self._gpu_statuses.clear()
        _LOG.debug("GPU Repo shutdown")

    def _initialize_devices(self) -> None:
        status = self._request_status()
        channel_info = ChannelInfo(SpeedOptions(
            # todo: build scripts for these options:
            profiles_enabled=False,
            fixed_enabled=True
        ))
        if status:
            self._gpu_statuses.append(DeviceStatus(
                # todo: adjust to handle multiple gpus (make device_id general)
                'gpu',
                status,
                _device_info=DeviceInfo(channels={'pump': channel_info, 'fan': channel_info})
            ))
        pass

    @staticmethod
    def _request_status() -> Optional[Status]:
        # todo: determine at start whether we have Nvidia or AMD
        # Nvidia:
        nvidia_gpus = GPUtil.getGPUs()
        if nvidia_gpus:
            gpu = nvidia_gpus[0]
            return Status(
                device_description=gpu.name,
                device_temperature=gpu.temperature,
                load_percent=gpu.load * 100
            )
        # AMD
        number_amd_gpus = pyamdgpuinfo.detect_gpus()
        if number_amd_gpus:
            gpu = pyamdgpuinfo.get_gpu(0)
            return Status(
                device_description=gpu.name,
                device_temperature=pyamdgpuinfo.query_temperature(),  # pylint: disable=no-member
                load_percent=pyamdgpuinfo.query_load()  # pylint: disable=no-member
            )
        return None
