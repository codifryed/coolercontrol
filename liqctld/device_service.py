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
from typing import List, Dict, Tuple, Any, Union

import liquidctl
from liquidctl.driver.base import BaseDriver

log = logging.getLogger(__name__)


class DeviceService:

    def __init__(self) -> None:
        self.devices: Dict[int, BaseDriver] = {}
        # this can be used to set specific flags like legacy/type/special things from settings in coolercontrol
        self.device_infos: Dict[int, Any] = {}

    def initialize_devices(self) -> List[Dict[str, Any]]:
        log.info("Initializing Liquidctl devices")
        try:
            found_devices = list(liquidctl.find_liquidctl_devices())
        except ValueError:  # ValueError can happen when no devices were found
            log.warning('No Liquidctl devices detected')
            return []
        # todo: check for legacy 690
        try:
            device_list: List[Dict[str, Any]] = []
            for index, lc_device in enumerate(found_devices):
                device_id: int = index + 1
                lc_device.connect()
                lc_init_status: List[Tuple] = lc_device.initialize()
                log.debug('Liquidctl device initialization response: %s', lc_init_status)
                self.devices[device_id] = lc_device
                device_list.append({
                    "id": device_id,
                    "description": lc_device.description,
                    "status": self._stringify_statuses(lc_init_status),
                    "device_type": type(lc_device).__name__
                })
            return device_list  # send ID, description and status
        except OSError as os_exc:  # OSError when device was found but there's a connection error (udev rules)
            log.error('Device Communication Error', exc_info=os_exc)
            return [{"error": "device communication"}]

    def get_status(self, device_id: str) -> List[Tuple]:
        log.debug(f"Getting status for device: {device_id}")
        try:
            statuses: List[Tuple] = self.devices[int(device_id)].get_status()
            log.debug(f"Status from Liquidctl: {statuses}")
            return self._stringify_statuses(statuses)
        except BaseException as err:
            log.error("Error getting status:", exc_info=err)
            return [("error", "device communication")]

    @staticmethod
    def _stringify_statuses(
            statuses: List[Tuple[str, Union[str, int, float], str]]
    ) -> List[Tuple[str, str, str]]:
        return [(str(status[0]), str(status[1]), str(status[2])) for status in statuses]
