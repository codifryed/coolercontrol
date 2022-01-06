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

from typing import List

from liquidctl.driver.base import BaseDriver
from liquidctl.driver.commander_pro import CommanderPro
from liquidctl.driver.kraken3 import KrakenX3, KrakenZ3

from models.device import Device
from repositories.test_mocks import TestMocks, COMMANDER_PRO_SAMPLE_RESPONSES, COMMANDER_PRO_SAMPLE_INITIALIZE_RESPONSES
from repositories.test_mocks import _MockKraken3Device, KRAKENX_SAMPLE_STATUS, KRAKENZ_SAMPLE_STATUS
from repositories.test_utils import Report, MockHidapiDevice
from settings import FeatureToggle


class TestRepoExtension:
    """These methods extend the current LiquidctlRepo for testing various configurations"""

    @staticmethod
    def insert_test_mocks(devices: List[BaseDriver]) -> None:
        if FeatureToggle.testing:
            # devices.clear()
            devices.extend([
                TestMocks.mockKrakenX2Device(),
                TestMocks.mockKrakenM2Device(),  # no cooling
                TestMocks.mockKrakenX3Device(),
                TestMocks.mockKrakenZ3Device(),  # mock issue with unsteady readings
                TestMocks.mockCommanderProDevice(),  # mock issue with getting status
            ])

    @staticmethod
    def prepare_for_mocks_get_status(device: Device, lc_device: BaseDriver) -> None:
        if FeatureToggle.testing:
            if isinstance(lc_device.device, _MockKraken3Device):
                if device.lc_driver_type is KrakenX3:
                    lc_device.device.preload_read(Report(0, KRAKENX_SAMPLE_STATUS))
                elif device.lc_driver_type is KrakenZ3:
                    lc_device.device.preload_read(Report(0, KRAKENZ_SAMPLE_STATUS))
                elif device.lc_driver_type is CommanderPro:
                    for response in COMMANDER_PRO_SAMPLE_RESPONSES:
                        lc_device.device.preload_read(Report(0, bytes.fromhex(response)))
                    lc_device._data.store('fan_modes', [0x01, 0x01, 0x02, 0x00, 0x00, 0x00])
                    lc_device._data.store('temp_sensors_connected', [0x01, 0x01, 0x00, 0x01])

    @staticmethod
    def connect_mock(lc_device: BaseDriver) -> None:
        if isinstance(lc_device.device, MockHidapiDevice) and isinstance(lc_device, CommanderPro):
            for response in COMMANDER_PRO_SAMPLE_INITIALIZE_RESPONSES:
                lc_device.device.preload_read(Report(0, bytes.fromhex(response)))
            for response in COMMANDER_PRO_SAMPLE_RESPONSES:
                lc_device.device.preload_read(Report(0, bytes.fromhex(response)))
            lc_device._data.store('fan_modes', [0x01, 0x01, 0x02, 0x00, 0x00, 0x00])
            lc_device._data.store('temp_sensors_connected', [0x01, 0x01, 0x00, 0x01])
        else:
            lc_device.connect()
