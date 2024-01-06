#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2023  Guy Boldon
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

from typing import List, Tuple

from coolercontrol_liqctld.device_service import E2E_TESTING_ENABLED
from coolercontrol_liqctld.e2e_tests.mocks import (
    _INIT_8297_SAMPLE,
    COMMANDER_PRO_SAMPLE_INITIALIZE_RESPONSES,
    COMMANDER_PRO_SAMPLE_RESPONSES,
    D5NEXT_SAMPLE_STATUS_REPORT,
    FARBWERK360_SAMPLE_STATUS_REPORT,
    H1V2_SAMPLE_STATUS,
    INIT_19AF_CONFIG,
    INIT_19AF_FIRMWARE,
    KRAKENX_SAMPLE_STATUS,
    KRAKENZ_SAMPLE_STATUS,
    OCTO_SAMPLE_STATUS_REPORT,
    QUADRO_SAMPLE_STATUS_REPORT,
    SMART_DEVICE_SAMPLE_RESPONSES,
    SMART_DEVICE_V2_SAMPLE_RESPONSE,
    Mock8297HidInterface,
    MockCommanderCoreDevice,
    TestMocks,
)
from coolercontrol_liqctld.e2e_tests.utils import (
    MockHidapiDevice,
    MockPyusbDevice,
    MockRuntimeStorage,
    Report,
)
from liquidctl.driver.aquacomputer import Aquacomputer
from liquidctl.driver.asetek import Legacy690Lc
from liquidctl.driver.aura_led import AuraLed
from liquidctl.driver.base import BaseDriver
from liquidctl.driver.commander_pro import CommanderPro
from liquidctl.driver.hydro_platinum import HydroPlatinum
from liquidctl.driver.kraken3 import KrakenX3
from liquidctl.driver.smart_device import H1V2, SmartDevice, SmartDevice2

if E2E_TESTING_ENABLED:
    from coolercontrol_liqctld.e2e_tests.mocks import MockKrakenZ3


class TestServiceExtension:
    """These methods extend the device_service for testing various configurations"""

    @staticmethod
    def insert_test_mocks(devices: List[BaseDriver]) -> None:
        if not E2E_TESTING_ENABLED:
            return
        devices.clear()
        devices.extend(
            [
                # TestMocks.mockAquacomputer_d5NextDevice(),
                # TestMocks.mockAquacomputer_Farbwerk360Device(),  # no speed channels
                # TestMocks.mockAquacomputer_OctoDevice(),
                # TestMocks.mockAquacomputer_QuadroDevice(),
                # TestMocks.mockAuraLed_19AFDevice(),
                # TestMocks.mock_commander_core_device(),
                # TestMocks.mockCommanderProDevice(),
                # TestMocks.mock_corsair_psu(),
                # TestMocks.mockH1V2(),
                # TestMocks.mockHydroPlatinumSeDevice(),  # throws checksum error but works
                # TestMocks.mockHydroPro(),  # has no mock response so fans don't show
                # TestMocks.mockKrakenX2Device(),
                # TestMocks.mockKrakenM2Device(),  # no cooling
                # TestMocks.mockKrakenX3Device(),
                # mock issue with unsteady readings, and lcd gives assertion errors -> many tries needed (can ignore bucket error):
                TestMocks.mockKrakenZ3Device(),
                # TestMocks.mockLegacy690LcDevice(),
                # TestMocks.mockModern690LcDevice(),
                # TestMocks.mockNzxtPsuDevice(),
                # TestMocks.mockRgbFusion2_8297Device(),
                # TestMocks.mockSmartDevice(),
                # TestMocks.mockSmartDevice2(),
            ]
        )

    @staticmethod
    def prepare_for_mocks_get_status(lc_device: BaseDriver) -> None:
        if not E2E_TESTING_ENABLED:
            return
        if isinstance(lc_device.device, MockHidapiDevice):
            device_type = type(lc_device)
            if device_type is KrakenX3:
                lc_device.device.preload_read(Report(0, KRAKENX_SAMPLE_STATUS))
            elif device_type is MockKrakenZ3:
                lc_device.device.preload_read(Report(0, KRAKENZ_SAMPLE_STATUS))
            elif device_type is CommanderPro:
                for response in COMMANDER_PRO_SAMPLE_RESPONSES:
                    lc_device.device.preload_read(Report(0, bytes.fromhex(response)))
                lc_device._data.store("fan_modes", [0x01, 0x01, 0x02, 0x00, 0x00, 0x00])
                lc_device._data.store(
                    "temp_sensors_connected", [0x01, 0x01, 0x00, 0x01]
                )
            elif device_type is H1V2:
                lc_device.device.preload_read(Report(0, H1V2_SAMPLE_STATUS))
            elif device_type is SmartDevice2:
                lc_device.device.preload_read(
                    Report(0, SMART_DEVICE_V2_SAMPLE_RESPONSE)
                )
            elif device_type is SmartDevice:
                for _, capdata in enumerate(SMART_DEVICE_SAMPLE_RESPONSES):
                    capdata = bytes.fromhex(capdata)
                    lc_device.device.preload_read(Report(capdata[0], capdata[1:]))
            elif device_type is AuraLed:
                lc_device.device.preload_read(INIT_19AF_CONFIG)
            elif device_type is Aquacomputer:
                aqua_type = lc_device._device_info["type"]
                if aqua_type == Aquacomputer._DEVICE_D5NEXT:
                    lc_device.device.preload_read(
                        Report(1, D5NEXT_SAMPLE_STATUS_REPORT)
                    )
                elif aqua_type == Aquacomputer._DEVICE_FARBWERK360:
                    lc_device.device.preload_read(
                        Report(1, FARBWERK360_SAMPLE_STATUS_REPORT)
                    )
                elif aqua_type == Aquacomputer._DEVICE_OCTO:
                    lc_device.device.preload_read(Report(1, OCTO_SAMPLE_STATUS_REPORT))
                elif aqua_type == Aquacomputer._DEVICE_QUADRO:
                    lc_device.device.preload_read(
                        Report(1, QUADRO_SAMPLE_STATUS_REPORT)
                    )
        elif isinstance(lc_device.device, MockCommanderCoreDevice):
            lc_device.device.speeds = (2357, 918, 903, 501, 1104, 1824, 104)
            lc_device.device.temperatures = (12.3, 45.6)
        elif isinstance(lc_device.device, MockPyusbDevice):
            pass

    @staticmethod
    def connect_mock(lc_device: BaseDriver) -> None:
        if not E2E_TESTING_ENABLED:
            return
        if isinstance(lc_device.device, MockHidapiDevice) and isinstance(
            lc_device, CommanderPro
        ):
            for response in COMMANDER_PRO_SAMPLE_INITIALIZE_RESPONSES:
                lc_device.device.preload_read(Report(0, bytes.fromhex(response)))
            for response in COMMANDER_PRO_SAMPLE_RESPONSES:
                lc_device.device.preload_read(Report(0, bytes.fromhex(response)))
            lc_device._data.store("fan_modes", [0x01, 0x01, 0x02, 0x00, 0x00, 0x00])
            lc_device._data.store("temp_sensors_connected", [0x01, 0x01, 0x00, 0x01])
        elif isinstance(lc_device.device, Mock8297HidInterface):
            lc_device.connect()
            lc_device.device.preload_read(_INIT_8297_SAMPLE)
        elif isinstance(lc_device.device, MockPyusbDevice) and isinstance(
            lc_device, Legacy690Lc
        ):
            runtime_storage = MockRuntimeStorage(key_prefixes=["testing"])
            runtime_storage.store("leds_enabled", 0)
            lc_device.connect(runtime_storage=runtime_storage)
        elif isinstance(lc_device.device, MockHidapiDevice) and isinstance(
            lc_device, HydroPlatinum
        ):
            runtime_storage = MockRuntimeStorage(key_prefixes=["testing"])
            runtime_storage.store("leds_enabled", 0)
            lc_device.connect(runtime_storage=runtime_storage)
        elif isinstance(lc_device.device, MockCommanderCoreDevice):
            lc_device.device.firmware_version = (0x01, 0x01, 0x01)
            lc_device.connect()
        else:
            lc_device.connect()

    @staticmethod
    def initialize_mock(lc_device: BaseDriver) -> List[Tuple]:
        if not E2E_TESTING_ENABLED:
            return []
        if isinstance(lc_device.device, MockHidapiDevice):
            if isinstance(lc_device, SmartDevice):
                for _, capdata in enumerate(SMART_DEVICE_SAMPLE_RESPONSES):
                    capdata = bytes.fromhex(capdata)
                    lc_device.device.preload_read(Report(capdata[0], capdata[1:]))
                return lc_device.initialize(direct_access=True)
            elif isinstance(lc_device, AuraLed):
                lc_device.device.preload_read(INIT_19AF_FIRMWARE)
                lc_device.device.preload_read(INIT_19AF_CONFIG)
                return lc_device.initialize()
        return lc_device.initialize()
