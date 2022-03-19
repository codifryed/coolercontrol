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
from typing import List, Optional

from coolero.models.device import Device, DeviceType
from coolero.models.device_info import DeviceInfo
from coolero.models.status import Status, TempStatus
from coolero.repositories.devices_repository import DevicesRepository
from coolero.settings import Settings

_LOG = logging.getLogger(__name__)
_ALL_AVG: str = 'All Average'


class CompositeRepo(DevicesRepository):
    """This class if for combining different temp sources from devices for lots of possibilities"""
    _composite_statuses: List[Device] = []

    def __init__(self, devices: List[Device]) -> None:
        self._devices = devices
        super().__init__()
        _LOG.info('Initialized with status: %s', self._composite_statuses)

    def _initialize_devices(self) -> None:
        if len(self._devices) < 2:
            return
        all_temps_status = self._get_avg_all_temps()
        if all_temps_status:
            self._composite_statuses.append(Device(
                _name=_ALL_AVG,
                _type_id=(DeviceType.COMPOSITE, len(self._composite_statuses) + 1),
                _status_current=all_temps_status,
                _colors={_ALL_AVG: Settings.theme['app_color']['white']},
                _info=DeviceInfo(temp_max=100, temp_ext_available=True)
            ))

    @property
    def statuses(self) -> List[Device]:
        return self._composite_statuses

    def update_statuses(self) -> None:
        for composite_device in self._composite_statuses:
            if composite_device.name == _ALL_AVG:
                composite_device.status = self._get_avg_all_temps()
                _LOG.debug('Composite device: %s status was updated with status: %s',
                           composite_device.name,
                           composite_device.status)

    def shutdown(self) -> None:
        self._composite_statuses.clear()
        _LOG.debug("Composite Repo shutdown")

    def _get_avg_all_temps(self) -> Optional[Status]:
        all_temps: List[float] = []
        for device in self._devices:
            if device.type != DeviceType.COMPOSITE:
                all_temps.extend(
                    temp_status.temp for temp_status in device.status.temps
                )
        return Status(
            temps=[TempStatus(_ALL_AVG, mean(all_temps), _ALL_AVG, _ALL_AVG)]
        ) if len(all_temps) > 1 else None
