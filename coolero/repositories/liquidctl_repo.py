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
from models.device import Device, DeviceType
from models.settings import Settings
from models.status import Status
from repositories.devices_repository import DevicesRepository
from services.device_extractor import DeviceExtractor

_LOG = logging.getLogger(__name__)


class LiquidctlRepo(DevicesRepository):
    """Repo for all Liquidctl devices"""

    _devices_drivers: Dict[int, Tuple[Device, BaseDriver]] = {}
    _device_info_extractor: DeviceExtractor

    def __init__(self) -> None:
        self._device_info_extractor = DeviceExtractor()
        super().__init__()
        _LOG.info('initialized with status: %s', self._devices_drivers)

    @property
    def statuses(self) -> List[Device]:
        return [device for device, _ in self._devices_drivers.values()]

    def update_statuses(self) -> None:
        for device, lc_device in self._devices_drivers.values():
            device.status = self._map_status(
                lc_device,
                lc_device.get_status()
            )
            _LOG.debug('Liquidctl device: %s status was updated with: %s',
                       device.device_name,
                       device.status)

    def shutdown(self) -> None:
        """Should be run on exit & shutdown, even in case of exception"""
        for _, lc_device in self._devices_drivers.values():
            lc_device.disconnect()
        self._devices_drivers.clear()
        _LOG.debug("Liquidctl Repo shutdown")

    def set_settings(self, lc_device_id: int, settings: Settings) -> None:
        _, lc_device = self._devices_drivers[lc_device_id]
        for channel, setting in settings.channel_settings.items():
            if setting.speed_fixed is not None:
                lc_device.set_fixed_speed(channel=channel, duty=setting.speed_fixed)
            elif setting.speed_profile:
                lc_device.set_speed_profile(channel=channel, profile=setting.speed_profile)
            elif setting.lighting is not None:
                lc_device.set_color(channel=channel, mode=setting.lighting.mode, colors=setting.lighting.colors)

    def _initialize_devices(self) -> None:
        _LOG.debug("Initializing Liquidctl devices")
        # todo: try catches all over, like for when no connection is possible
        devices = liquidctl.find_liquidctl_devices()

        for index, lc_device in enumerate(devices):
            lc_device.connect()
            lc_init_status: List[Tuple] = lc_device.initialize()
            _LOG.debug(f'Liquidctl device initialization response: {lc_init_status}')
            init_status = self._map_status(lc_device, lc_init_status)
            device_info = self._extract_device_info(lc_device)
            device = Device(
                _device_name=lc_device.description,
                _device_type=DeviceType.LIQUIDCTL,
                _status_current=init_status,
                _lc_device_id=index,
                _lc_driver_type=type(lc_device),
                _lc_init_firmware_version=init_status.firmware_version,
                _device_info=device_info
            )
            # get the status after initialization to fill with complete data right away
            device.status = self._map_status(lc_device, lc_device.get_status())
            self._devices_drivers[index] = (device, lc_device)

    def _map_status(self, device: BaseDriver, lc_status: List[Tuple]) -> Status:
        status_dict = self._convert_status_to_dict(lc_status)
        return self._device_info_extractor.extract_status_from(device, status_dict)

    @staticmethod
    def _convert_status_to_dict(lc_status: List[Tuple]) -> Dict[str, Union[str, int, float]]:
        return {
            str(lc_property).strip().lower(): value
            for lc_property, value, unit in lc_status
        }

    def _extract_device_info(self, device: BaseDriver) -> Optional[DeviceInfo]:
        return self._device_info_extractor.extract_info_from(device)
