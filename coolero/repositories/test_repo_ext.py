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
from liquidctl.driver.kraken3 import KrakenX3, KrakenZ3

from models.device import Device
from repositories.test_mocks import TestMocks
from repositories.test_mocks import _MockKraken3Device, KRAKENX_SAMPLE_STATUS, KRAKENZ_SAMPLE_STATUS
from repositories.test_utils import Report
from settings import FeatureToggle


class TestRepoExtension:
    """These methods extend the current LiquidctlRepo for testing various configurations"""

    @staticmethod
    def insert_test_mocks(devices: List[BaseDriver]) -> None:
        if FeatureToggle.testing:
            # devices.clear()
            devices.extend([
                TestMocks.mockKrakenX2Device(),
                TestMocks.mockKrakenX3Device(),
                TestMocks.mockKrakenZ3Device(),
            ])

    @staticmethod
    def prepare_for_mocks_get_status(device: Device, lc_device: BaseDriver) -> None:
        if FeatureToggle.testing:
            if isinstance(lc_device.device, _MockKraken3Device):
                if device.lc_driver_type is KrakenX3:
                    lc_device.device.preload_read(Report(0, KRAKENX_SAMPLE_STATUS))
                elif device.lc_driver_type is KrakenZ3:
                    lc_device.device.preload_read(Report(0, KRAKENZ_SAMPLE_STATUS))
