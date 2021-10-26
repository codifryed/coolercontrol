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

import logging.config
from typing import Optional, List, Dict, Tuple, Any, Union

import liquidctl
from liquidctl.driver.base import BaseDriver

from models.device_info import DeviceInfo
from models.device_status import DeviceStatus
from models.status import Status
from repositories.devices_repository import DevicesRepository
from services.device_extractor import DeviceExtractor

_LOG = logging.getLogger(__name__)


class LiquidctlRepo(DevicesRepository):
    """Repo for all Liquidctl devices"""

    _device_statuses: Dict[int, Tuple[DeviceStatus, BaseDriver]] = {}
    _device_info_extractor: DeviceExtractor

    def __init__(self) -> None:
        self._device_info_extractor = DeviceExtractor()
        super().__init__()
        _LOG.info('initialized with status: %s', self._device_statuses)

    @property
    def statuses(self) -> List[DeviceStatus]:
        return [device_status for device_status, _ in self._device_statuses.values()]

    def update_statuses(self) -> None:
        for device_status, lc_device in self._device_statuses.values():
            device_status.status = self._map_status(
                lc_device,
                lc_device.get_status()
            )
            _LOG.debug('Liquidctl device: %s status was updated with: %s',
                       device_status.device_name,
                       device_status.status)

    def shutdown(self) -> None:
        """Should be run on exit & shutdown, even in case of exception"""
        for _, lc_device in self._device_statuses.values():
            lc_device.disconnect()
        self._device_statuses.clear()
        _LOG.debug("Liquidctl Repo shutdown")

    def _initialize_devices(self) -> None:
        _LOG.debug("Initializing Liquidctl devices")
        # todo: try catches all over, like for when no connection is possible
        devices = liquidctl.find_liquidctl_devices()

        for index, device in enumerate(devices):
            device.connect()
            lc_init_status: List[Tuple] = device.initialize()
            _LOG.debug(f'Liquidctl device initialization response: {lc_init_status}')
            init_status = self._map_status(device, lc_init_status)
            device_info = self._extract_device_info(device)
            device_status = DeviceStatus(
                _device_name=device.description,
                _status_current=init_status,
                _lc_device_id=index,
                _lc_driver_type=type(device),
                _lc_init_firmware_version=init_status.firmware_version,
                _device_info=device_info
            )
            # get the status after initialization to fill with complete data right away
            device_status.status = self._map_status(device, device.get_status())
            self._device_statuses[index] = (device_status, device)

    def _map_status(self, device: BaseDriver, lc_status: List[Tuple]) -> Status:
        status_dict = self._convert_status_to_dict(lc_status)
        return self._device_info_extractor.extract_status_from(device, status_dict)

    @staticmethod
    def _convert_status_to_dict(lc_status: List[Tuple]) -> Dict[str, Union[str, int, float]]:
        return dict((str(lc_property).strip().lower(), value) for lc_property, value, unit in lc_status)

    def _extract_device_info(self, device: BaseDriver) -> Optional[DeviceInfo]:
        return self._device_info_extractor.extract_info_from(device)
