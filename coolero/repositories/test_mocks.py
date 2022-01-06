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

# These are modified from liquidctl testing: https://github.com/liquidctl/liquidctl
from liquidctl.driver.commander_pro import CommanderPro
from liquidctl.driver.kraken2 import Kraken2

from repositories.test_utils import MockHidapiDevice, Report, MockRuntimeStorage

from liquidctl.driver.kraken3 import KrakenX3, KrakenZ3
from liquidctl.driver.kraken3 import _COLOR_CHANNELS_KRAKENX
from liquidctl.driver.kraken3 import _SPEED_CHANNELS_KRAKENX
from liquidctl.driver.kraken3 import _SPEED_CHANNELS_KRAKENZ
from liquidctl.util import HUE2_MAX_ACCESSORIES_IN_CHANNEL as MAX_ACCESSORIES
from liquidctl.util import Hue2Accessory

KRAKENX_SAMPLE_STATUS = bytes.fromhex(
    '7502200036000b51535834353320012101a80635350000000000000000000000'
    '0000000000000000000000000000000000000000000000000000000000000000'
)

KRAKENZ_SAMPLE_STATUS = bytes.fromhex(
    '7501160037000a51383430353132011e043b0732320100a20328280000000000'
    '0000000000000000000000000000000000000000000000000000000000000000'
)

COMMANDER_PRO_SAMPLE_INITIALIZE_RESPONSES = [
    '000009d4000000000000000000000000',  # firmware
    '00000500000000000000000000000000',  # bootloader
    '00010100010000000000000000000000',  # temp probes
    '00010102000000000000000000000000'  # fan probes
]

COMMANDER_PRO_SAMPLE_RESPONSES = [
    '000a8300000000000000000000000000',  # temp sensor 1
    '000b6a00000000000000000000000000',  # temp sensor 2
    '000a0e00000000000000000000000000',  # temp sensor 4
    '0003ac00000000000000000000000000',  # fan speed 1
    '0003ab00000000000000000000000000',  # fan speed 2
    '0003db00000000000000000000000000',  # fan speed 3
    '002f2200000000000000000000000000',  # get 12v
    '00136500000000000000000000000000',  # get 5v
    '000d1f00000000000000000000000000',  # get 3.3v
]


class TestMocks:

    ####################################################################################################################
    # Kraken 2

    @staticmethod
    def mockKrakenX2Device() -> Kraken2:
        device = _MockKraken2Device(fw_version=(6, 0, 2))
        return Kraken2(device, 'NZXT Kraken X (X42, X52, X62 or X72)', device_type=Kraken2.DEVICE_KRAKENX)

    @staticmethod
    def mockKrakenM2Device() -> Kraken2:
        device = _MockKraken2Device(fw_version=(6, 0, 2))
        return Kraken2(device, 'NZXT Kraken M22', device_type=Kraken2.DEVICE_KRAKENM)

    ####################################################################################################################
    # Kraken 3

    @staticmethod
    def mockKrakenX3Device() -> KrakenX3:
        device = _MockKraken3Device(raw_led_channels=len(_COLOR_CHANNELS_KRAKENX) - 1)
        return KrakenX3(
            device, 'NZXT Kraken X (X53, X63 or X73)',
            speed_channels=_SPEED_CHANNELS_KRAKENX,
            color_channels=_COLOR_CHANNELS_KRAKENX
        )

    @staticmethod
    def mockKrakenZ3Device() -> KrakenZ3:
        device = _MockKraken3Device(raw_led_channels=0)
        return KrakenZ3(device, 'NZXT Kraken Z (Z53, Z63 or Z73) (experimental)',
                        speed_channels=_SPEED_CHANNELS_KRAKENZ,
                        color_channels={})

    ####################################################################################################################
    # Corsair Commander Pro

    @staticmethod
    def mockCommanderProDevice() -> CommanderPro:
        device = MockHidapiDevice(vendor_id=0x1b1c, product_id=0x0c10, address='addr')
        pro = CommanderPro(device, 'Corsair Commander Pro', 6, 4, 2)
        runtime_storage = MockRuntimeStorage(key_prefixes=['testing'])
        pro.connect(runtime_storage=runtime_storage)
        return pro

class _MockKraken2Device(MockHidapiDevice):
    def __init__(self, fw_version):
        super().__init__(vendor_id=0xffff, product_id=0x1e71)
        self.fw_version = fw_version
        self.temperature = 30.9
        self.fan_speed = 1499
        self.pump_speed = 2702

    def read(self, length):
        pre = super().read(length)
        if pre:
            return pre
        buf = bytearray(64)
        buf[1:3] = divmod(int(self.temperature * 10), 10)
        buf[3:5] = self.fan_speed.to_bytes(length=2, byteorder='big')
        buf[5:7] = self.pump_speed.to_bytes(length=2, byteorder='big')
        major, minor, patch = self.fw_version
        buf[0xb] = major
        buf[0xc:0xe] = minor.to_bytes(length=2, byteorder='big')
        buf[0xe] = patch
        return buf[:length]


class _MockKraken3Device(MockHidapiDevice):
    def __init__(self, raw_led_channels):
        super().__init__()
        self.raw_led_channels = raw_led_channels

    def write(self, data):
        reply = bytearray(64)
        if data[0:2] == [0x10, 0x01]:
            reply[0:2] = [0x11, 0x01]
        elif data[0:2] == [0x20, 0x03]:
            reply[0:2] = [0x21, 0x03]
            reply[14] = self.raw_led_channels
            if self.raw_led_channels > 1:
                reply[15 + 1 * MAX_ACCESSORIES] = Hue2Accessory.KRAKENX_GEN4_RING.value
                reply[15 + 2 * MAX_ACCESSORIES] = Hue2Accessory.KRAKENX_GEN4_LOGO.value
        self.preload_read(Report(0, reply))
