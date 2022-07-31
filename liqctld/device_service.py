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
from typing import List, Dict, Tuple, Any

import liquidctl
from liquidctl.driver.base import BaseDriver

log = logging.getLogger(__name__)


class DeviceService:

    def __init__(self) -> None:
        self.devices: List[BaseDriver] = []
        # this can be used to set specific flags like legacy/type/special things from settings in coolercontrol
        self.device_infos: Dict[int, Any] = {}

    def initialize_devices(self) -> Dict:
        log.info("Initializing Liquidctl devices")
        try:
            self.devices = list(liquidctl.find_liquidctl_devices())
        except ValueError:  # ValueError can happen when no devices were found
            log.warning('No Liquidctl devices detected')
            return {"devices": {}}
        try:
            device_dict: Dict[int, Dict[str, Any]] = {}
            for index, lc_device in enumerate(self.devices):
                lc_device.connect()
                lc_init_status: List[Tuple] = lc_device.initialize()
                log.debug('Liquidctl device initialization response: %s', lc_init_status)
                device_dict[index + 1] = {"description": lc_device.description, "status": lc_init_status}
            return device_dict  # send ID, description and status
        except OSError as os_exc:  # OSError when device was found but there's a connection error (udev rules)
            log.error('Device Communication Error', exc_info=os_exc)
            return {"error": "device communication"}
