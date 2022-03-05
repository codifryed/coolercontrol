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
from collections import deque

from liquidctl.driver.asetek import Modern690Lc
from liquidctl.driver.asetek_pro import CorsairAsetekProDriver
from liquidctl.driver.commander_core import CommanderCore
from liquidctl.driver.commander_pro import CommanderPro
from liquidctl.driver.corsair_hid_psu import CorsairHidPsu
from liquidctl.driver.hydro_platinum import HydroPlatinum
from liquidctl.driver.kraken2 import Kraken2
from liquidctl.driver.kraken3 import KrakenX3, KrakenZ3
from liquidctl.driver.kraken3 import _COLOR_CHANNELS_KRAKENX
from liquidctl.driver.kraken3 import _SPEED_CHANNELS_KRAKENX
from liquidctl.driver.kraken3 import _SPEED_CHANNELS_KRAKENZ
from liquidctl.driver.nzxt_epsu import NzxtEPsu
from liquidctl.driver.rgb_fusion2 import RgbFusion2
from liquidctl.driver.smart_device import SmartDevice2, SmartDevice
from liquidctl.pmbus import compute_pec
from liquidctl.util import HUE2_MAX_ACCESSORIES_IN_CHANNEL as MAX_ACCESSORIES, u16le_from
from liquidctl.util import Hue2Accessory

from coolero.repositories.test_utils import MockHidapiDevice, Report, MockRuntimeStorage, MockPyusbDevice, noop

########################################################################################################################
# Sample Responses:

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

SMART_DEVICE_V2_SAMPLE_RESPONSE = bytes.fromhex(
    '67023a003f00185732533230312003000100000000000000ff03000000000000'
    '0000000000000000323232000000000032323200000000003000000000000000'
)

SMART_DEVICE_SAMPLE_RESPONSES = [
    '043e00056e00000b5b000301000007200002001e00',
    '04400005b500000b5b000201000007020002001e00',
    '044000053800000b5b000201000007120102001e00',
]

LEGACY690LC_DEVICE_SAMPLE_RESPONSE = bytes.fromhex(
    '0348feeb125f7cf709602812ff5c0118'
    'e718feeb20dd0000070000d347806711'
)

_INIT_8297_DATA = bytes.fromhex(
    '00010001010006000000000049543832393742582d4742583537300000000000'
    '0000000000000000000000000200010002000100000102000001978200000000'
)
_INIT_8297_SAMPLE = Report(_INIT_8297_DATA[0], _INIT_8297_DATA[1:])

CORSAIR_SAMPLE_PAGED_RESPONSES = [
    [
        '038bffd2',
        '038c2bf0',
        '03963e08',
    ],
    [
        '038b41d1',
        '038c1be0',
        '039610f8',
    ],
    [
        '038bd3d0',
        '038c09e0',
        '039603f8',
    ],
]

CORSAIR_SAMPLE_RESPONSES = [
    '033b1b',
    '034013d1',
    '03441ad2',
    '034680e2',
    '034f46',
    '0388ccf9',
    '038d86f0',
    '038e6af0',
    '0399434f5253414952',
    '039a524d3130303069',
    '03d46d9febfe',
    '03d802',
    '03ee4608',
    'fe03524d3130303069',

    '03d29215',
    '03d1224711',

    # artificial
    '0390c803',
    '03f001',
]

HYDRO_PLATINUM_SAMPLE_PATH = (r'IOService:/AppleACPIPlatformExpert/PCI0@0/AppleACPIPCI/XHC@14/XH'
                              r'C@14000000/HS11@14a00000/USB2.0 Hub@14a00000/AppleUSB20InternalH'
                              r'ub@14a00000/AppleUSB20HubPort@14a10000/USB2.0 Hub@14a10000/Apple'
                              r'USB20Hub@14a10000/AppleUSB20HubPort@14a12000/H100i Platinum@14a1'
                              r'2000/IOUSBHostInterface@0/AppleUserUSBHostHIDDevice+Win\\#!&3142')


class TestMocks:
    """Test Mock Instance Factory"""

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

    ####################################################################################################################
    # NZXT Smart Device V2

    @staticmethod
    def mockSmartDevice2() -> SmartDevice2:
        device = _MockSmartDevice2(raw_speed_channels=3, raw_led_channels=2)
        return SmartDevice2(device, 'NZXT Smart Device V2', speed_channel_count=3, color_channel_count=2)

    ####################################################################################################################
    # NZXT Smart Device V1

    @staticmethod
    def mockSmartDevice() -> SmartDevice:
        device = MockHidapiDevice(vendor_id=0x1e71, product_id=0x1714, address='addr')
        return SmartDevice(device, 'NZXT Smart Device V1', speed_channel_count=3, color_channel_count=1)

    ####################################################################################################################
    # Modern Asetek:
    # (EVGA CLC 120 (CLC12), 240, 280 and 360,
    # Corsair Hydro H80i v2, H100i v2 and H115i,
    # Corsair Hydro H80i GT, H100i GTX and H110i GTX)

    @staticmethod
    def mockModern690LcDevice() -> Modern690Lc:
        device = MockPyusbDevice()
        return Modern690Lc(device, 'Modern 690LC - EVGA, Corsair')

    ####################################################################################################################
    # Legacy Asetek: (NZXT Kraken X40, X60, X31, X41, X51 and X61)

    @staticmethod
    def mockLegacy690LcDevice() -> Modern690Lc:
        device = MockPyusbDevice(vendor_id=0xffff, product_id=0xb200, bus=1, port=(1,))
        # return Legacy690Lc(device, 'NZXT Kraken X60')
        # the legacy devices are detected as moderns at first and user has to select if this is a legacy or not.
        return Modern690Lc(device, 'NZXT Kraken X60')

    ####################################################################################################################
    # ITE 8297: found in Gigabyte X570 Aorus Elite - RGB Fusion

    @staticmethod
    def mockRgbFusion2_8297Device() -> RgbFusion2:
        device = Mock8297HidInterface(vendor_id=0x048d, product_id=0x8297, address='addr')
        return RgbFusion2(device, 'Gigabyte RGB Fusion 2.0 8297 Controller')

    ####################################################################################################################
    # Corsair HID PSU

    @staticmethod
    def mock_corsair_psu() -> CorsairHidPsu:
        pid, vid, _, desc, kwargs = CorsairHidPsu.SUPPORTED_DEVICES[0]
        device = MockCorsairPsu(vendor_id=vid, product_id=pid, address='addr')
        return CorsairHidPsu(device, desc, **kwargs)

    ####################################################################################################################
    # NZXT E PSU

    @staticmethod
    def mockNzxtPsuDevice() -> NzxtEPsu:
        device = _MockNzxtPsuDevice()
        return NzxtEPsu(device, 'NZXT E500 PSU')

    ####################################################################################################################
    # AseTek Pro

    @staticmethod
    def mockHydroPro() -> CorsairAsetekProDriver:
        usb_dev = MockPyusbDevice()
        return CorsairAsetekProDriver(usb_dev, 'Asetek Pro cooler', fan_count=2)

    ####################################################################################################################
    # Hydro Platinum - choose the model with the most features to mock

    @staticmethod
    def mockHydroPlatinumSeDevice() -> HydroPlatinum:
        description = 'H100i Platinum SE'
        kwargs = {'fan_count': 2, 'fan_leds': 16}
        device = _MockHydroPlatinumDevice()
        return HydroPlatinum(device, description, **kwargs)

    ####################################################################################################################
    # Corsair Commander Core & Corsair iCUE

    @staticmethod
    def mock_commander_core_device() -> CommanderCore:
        device = MockCommanderCoreDevice()
        return CommanderCore(device, 'Corsair Commander Core (experimental)')


########################################################################################################################
# Mock Class Helpers:


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


class _MockSmartDevice2(MockHidapiDevice):
    def __init__(self, raw_speed_channels, raw_led_channels):
        super().__init__()
        self.raw_speed_channels = raw_speed_channels
        self.raw_led_channels = raw_led_channels

    def write(self, data):
        reply = bytearray(64)
        if data[0:2] == [0x10, 0x01]:
            reply[0:2] = [0x11, 0x01]
        elif data[0:2] == [0x20, 0x03]:
            reply[0:2] = [0x21, 0x03]
            reply[14] = self.raw_led_channels
            if self.raw_led_channels > 1:
                reply[15 + 1 * 6] = 0x10
                reply[15 + 2 * 6] = 0x11
        self.preload_read(Report(reply[0], reply[1:]))


class Mock8297HidInterface(MockHidapiDevice):
    def get_feature_report(self, report_id, length):
        """Get a feature report emulating out of spec behavior of the device."""
        return super().get_feature_report(0, length)


class MockCorsairPsu(MockHidapiDevice):
    def __init__(self, *args, **kwargs):
        self._page = 0;
        super().__init__(*args, **kwargs)

    def write(self, data):
        super().write(data)
        data = data[1:]  # skip unused report ID

        reply = bytearray(64)

        if data[0] == 2 and data[1] == 0:
            self._page = data[2]
            reply[0:3] = data[0:3]
            self.preload_read(Report(0, reply))
        else:
            cmd = f'{data[1]:02x}'
            samples = [x for x in CORSAIR_SAMPLE_PAGED_RESPONSES[self._page] if x[2:4] == cmd]
            if not samples:
                samples = [x for x in CORSAIR_SAMPLE_RESPONSES if x[2:4] == cmd]
            if not samples:
                raise KeyError(cmd)
            reply[0:len(data)] = bytes.fromhex(samples[0])
            self.preload_read(Report(0, reply))


class _MockNzxtPsuDevice(MockHidapiDevice):
    def write(self, data):
        super().write(data)
        data = data[1:]  # skip unused report ID
        reply = bytearray(64)
        reply[0:2] = (0xaa, data[2])
        if data[5] == 0x06:
            reply[2] = data[2] - 2
        elif data[5] == 0xfc:
            reply[2:4] = (0x11, 0x41)
        self.preload_read(Report(0, reply[0:]))


class _MockHydroPlatinumDevice(MockHidapiDevice):
    def __init__(self):
        super().__init__(vendor_id=0xffff, product_id=0x0c17, address=HYDRO_PLATINUM_SAMPLE_PATH)
        self.fw_version = (1, 1, 15)
        self.temperature = 30.9
        self.fan1_speed = 1499
        self.fan2_speed = 1512
        self.fan3_speed = 1777
        self.pump_speed = 2702

    def read(self, length):
        pre = super().read(length)
        if pre:
            return pre
        buf = bytearray(64)
        buf[2] = self.fw_version[0] << 4 | self.fw_version[1]
        buf[3] = self.fw_version[2]
        buf[7] = int((self.temperature - int(self.temperature)) * 255)
        buf[8] = int(self.temperature)
        buf[14] = round(.10 * 255)
        buf[15:17] = self.fan1_speed.to_bytes(length=2, byteorder='little')
        buf[21] = round(.20 * 255)
        buf[22:24] = self.fan2_speed.to_bytes(length=2, byteorder='little')
        buf[28] = round(.70 * 255)
        buf[29:31] = self.pump_speed.to_bytes(length=2, byteorder='little')
        buf[42] = round(.30 * 255)
        buf[43:44] = self.fan3_speed.to_bytes(length=2, byteorder='little')
        buf[-1] = compute_pec(buf[1:-1])
        return buf[:length]


def int_to_le(num, length=2, byteorder='little', signed=False):
    """Helper method for the MockCommanderCoreDevice"""
    return int(num).to_bytes(length=length, byteorder=byteorder, signed=signed)


class MockCommanderCoreDevice:
    def __init__(self):
        self.vendor_id = 0x1b1c
        self.product_id = 0x0c1c
        self.address = 'addr'
        self.path = b'path'
        self.release_number = None
        self.serial_number = None
        self.bus = None
        self.port = None

        self.open = noop
        self.close = noop
        self.clear_enqueued_reports = noop

        self._read = deque()
        self.sent = list()

        self._last_write = bytes()
        self._modes = {}

        self._awake = False

        self.response_prefix = ()
        self.firmware_version = (0x00, 0x00, 0x00)
        self.led_counts = (None, None, None, None, None, None, None)
        self.speeds_mode = (0, 0, 0, 0, 0, 0, 0)
        self.speeds = (None, None, None, None, None, None, None)
        self.fixed_speeds = (0, 0, 0, 0, 0, 0, 0)
        self.temperatures = (None, None)

    def read(self, length):
        data = bytearray([0x00, self._last_write[2], 0x00])
        data.extend(self.response_prefix)

        if self._last_write[2] == 0x02:  # Firmware version
            for i in range(0, 3):
                data.append(self.firmware_version[i])
        if self._awake:
            if self._last_write[2] == 0x08:  # Get data
                channel = self._last_write[3]
                mode = self._modes[channel]
                if mode[1] == 0x00:
                    if mode[0] == 0x17:  # Get speeds
                        data.extend([0x06, 0x00])
                        data.append(len(self.speeds))
                        for i in self.speeds:
                            if i is None:
                                data.extend([0x00, 0x00])
                            else:
                                data.extend(int_to_le(i))
                    elif mode[0] == 0x1a:  # Speed devices connected
                        data.extend([0x09, 0x00])
                        data.append(len(self.speeds))
                        for i in self.speeds:
                            data.extend([0x01 if i is None else 0x07])
                    elif mode[0] == 0x20:  # LED detect
                        data.extend([0x0f, 0x00])
                        data.append(len(self.led_counts))
                        for i in self.led_counts:
                            if i is None:
                                data.extend(int_to_le(3) + int_to_le(0))
                            else:
                                data.extend(int_to_le(2))
                                data.extend(int_to_le(i))
                    elif mode[0] == 0x21:  # Get temperatures
                        data.extend([0x10, 0x00])
                        data.append(len(self.temperatures))
                        for i in self.temperatures:
                            if i is None:
                                data.append(1)
                                data.extend(int_to_le(0))
                            else:
                                data.append(0)
                                data.extend(int_to_le(int(i * 10)))
                    else:
                        raise NotImplementedError(f'Read for {mode.hex(":")}')
                elif mode[1] == 0x6d:
                    if mode[0] == 0x60:
                        data.extend([0x03, 0x00])
                        data.append(len(self.speeds_mode))
                        for i in self.speeds_mode:
                            data.append(i)
                    elif mode[0] == 0x61:
                        data.extend([0x04, 0x00])
                        data.append(len(self.fixed_speeds))
                        for i in self.fixed_speeds:
                            data.extend(int_to_le(i))
                    else:
                        raise NotImplementedError(f'Read for {mode.hex(":")}')
                else:
                    raise NotImplementedError(f'Read for {mode.hex(":")}')

        return list(data)[:length]

    def write(self, data):
        data = bytes(data)  # ensure data is convertible to bytes
        self._last_write = data
        if data[0] != 0x00 or data[1] != 0x08:
            raise ValueError('Start of packets going out should be 00:08')

        if data[2] == 0x0d:
            channel = data[3]
            if self._modes[channel] is None:
                self._modes[channel] = data[4:6]
        elif data[2] == 0x05 and data[3] == 0x01:
            self._modes[data[4]] = None
        elif data[2] == 0x01 and data[3] == 0x03 and data[4] == 0x00:
            self._awake = data[5] == 0x02
        elif self._awake:
            if data[2] == 0x06:  # Write command
                channel = data[3]
                mode = self._modes[channel]
                length = u16le_from(data[4:6])
                data_type = data[8:10]
                written_data = data[10:8 + length]
                if mode[1] == 0x6d:
                    if mode[0] == 0x60 and list(data_type) == [0x03, 0x00]:
                        self.speeds_mode = tuple(written_data[i + 1] for i in range(0, written_data[0]))
                    elif mode[0] == 0x61 and list(data_type) == [0x04, 0x00]:
                        self.fixed_speeds = tuple(
                            u16le_from(written_data[i * 2 + 1:i * 2 + 3]) for i in range(0, written_data[0]))
                    else:
                        raise NotImplementedError('Invalid Write command')
                else:
                    raise NotImplementedError('Invalid Write command')

        return len(data)
