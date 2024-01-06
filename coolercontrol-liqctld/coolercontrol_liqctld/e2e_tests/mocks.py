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

# These are modified from liquidctl testing: https://github.com/liquidctl/liquidctl
from collections import deque

from coolercontrol_liqctld.device_service import E2E_TESTING_ENABLED
from coolercontrol_liqctld.e2e_tests.utils import (
    MockHidapiDevice,
    MockPyusbDevice,
    MockRuntimeStorage,
    Report,
    noop,
)
from liquidctl import ExpectationNotMet
from liquidctl.driver.aquacomputer import Aquacomputer
from liquidctl.driver.asetek import Modern690Lc
from liquidctl.driver.asetek_pro import HydroPro
from liquidctl.driver.aura_led import AuraLed
from liquidctl.driver.commander_core import CommanderCore
from liquidctl.driver.commander_pro import CommanderPro
from liquidctl.driver.corsair_hid_psu import CorsairHidPsu
from liquidctl.driver.hydro_platinum import HydroPlatinum
from liquidctl.driver.kraken2 import Kraken2
from liquidctl.driver.kraken3 import (
    _COLOR_CHANNELS_KRAKENX,
    _COLOR_CHANNELS_KRAKENZ,
    _HWMON_CTRL_MAPPING_KRAKENZ,
    _SPEED_CHANNELS_KRAKENX,
    _SPEED_CHANNELS_KRAKENZ,
    KrakenX3,
    KrakenZ3,
)
from liquidctl.driver.nzxt_epsu import NzxtEPsu
from liquidctl.driver.rgb_fusion2 import RgbFusion2
from liquidctl.driver.smart_device import H1V2, SmartDevice, SmartDevice2
from liquidctl.pmbus import compute_pec
from liquidctl.util import HUE2_MAX_ACCESSORIES_IN_CHANNEL as MAX_ACCESSORIES
from liquidctl.util import Hue2Accessory, u16le_from

########################################################################################################################
# Sample Responses:

KRAKENX_SAMPLE_STATUS = bytes.fromhex(
    "7502200036000b51535834353320012101a80635350000000000000000000000"
    "0000000000000000000000000000000000000000000000000000000000000000"
)

KRAKENZ_SAMPLE_STATUS = bytes.fromhex(
    "7501160037000a51383430353132011e043b0732320100a20328280000000000"
    "0000000000000000000000000000000000000000000000000000000000000000"
)

COMMANDER_PRO_SAMPLE_INITIALIZE_RESPONSES = [
    "000009d4000000000000000000000000",  # firmware
    "00000500000000000000000000000000",  # bootloader
    "00010100010000000000000000000000",  # temp probes
    "00010102000000000000000000000000",  # fan probes
]

COMMANDER_PRO_SAMPLE_RESPONSES = [
    "000a8300000000000000000000000000",  # temp sensor 1
    "000b6a00000000000000000000000000",  # temp sensor 2
    "000a0e00000000000000000000000000",  # temp sensor 4
    "0003ac00000000000000000000000000",  # fan speed 1
    "0003ab00000000000000000000000000",  # fan speed 2
    "0003db00000000000000000000000000",  # fan speed 3
    "002f2200000000000000000000000000",  # get 12v
    "00136500000000000000000000000000",  # get 5v
    "000d1f00000000000000000000000000",  # get 3.3v
]

H1V2_SAMPLE_STATUS = bytes.fromhex(
    "75021320020d85bcabab94188f5f010000a00f0032020284021e1e02f9066464"
    "0000000000000000000000000000000000000000000000000000000000000005"
)

SMART_DEVICE_V2_SAMPLE_RESPONSE = bytes.fromhex(
    "67023a003f00185732533230312003000100000000000000ff03000000000000"
    "0000000000000000323232000000000032323200000000003000000000000000"
)

SMART_DEVICE_SAMPLE_RESPONSES = [
    "043e00056e00000b5b000301000007200002001e00",
    "04400005b500000b5b000201000007020002001e00",
    "044000053800000b5b000201000007120102001e00",
]

LEGACY690LC_DEVICE_SAMPLE_RESPONSE = bytes.fromhex(
    "0348feeb125f7cf709602812ff5c0118" "e718feeb20dd0000070000d347806711"
)

_INIT_8297_DATA = bytes.fromhex(
    "00010001010006000000000049543832393742582d4742583537300000000000"
    "0000000000000000000000000200010002000100000102000001978200000000"
)
_INIT_8297_SAMPLE = Report(_INIT_8297_DATA[0], _INIT_8297_DATA[1:])

CORSAIR_SAMPLE_PAGED_RESPONSES = [
    [
        "038bffd2",
        "038c2bf0",
        "03963e08",
    ],
    [
        "038b41d1",
        "038c1be0",
        "039610f8",
    ],
    [
        "038bd3d0",
        "038c09e0",
        "039603f8",
    ],
]

CORSAIR_SAMPLE_RESPONSES = [
    "033b1b",
    "034013d1",
    "03441ad2",
    "034680e2",
    "034f46",
    "0388ccf9",
    "038d86f0",
    "038e6af0",
    "0399434f5253414952",
    "039a524d3130303069",
    "03d46d9febfe",
    "03d802",
    "03ee4608",
    "fe03524d3130303069",
    "03d29215",
    "03d1224711",
    # artificial
    "0390c803",
    "03f001",
]

HYDRO_PLATINUM_SAMPLE_PATH = (
    r"IOService:/AppleACPIPlatformExpert/PCI0@0/AppleACPIPCI/XHC@14/XH"
    r"C@14000000/HS11@14a00000/USB2.0 Hub@14a00000/AppleUSB20InternalH"
    r"ub@14a00000/AppleUSB20HubPort@14a10000/USB2.0 Hub@14a10000/Apple"
    r"USB20Hub@14a10000/AppleUSB20HubPort@14a12000/H100i Platinum@14a1"
    r"2000/IOUSBHostInterface@0/AppleUserUSBHostHIDDevice+Win\\#!&3142"
)

_INIT_19AF_FIRMWARE_DATA = bytes.fromhex(
    "ec0241554c41332d415233322d30323037000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000"
)
INIT_19AF_FIRMWARE = Report(_INIT_19AF_FIRMWARE_DATA[0], _INIT_19AF_FIRMWARE_DATA[1:])

_INIT_19AF_CONFIG_DATA = bytes.fromhex(
    "ec3000001e9f03010000783c00010000783c00010000783c0000000000000001"
    "040201f40000000000000000000000000000000000000000000000000000000000"
)
INIT_19AF_CONFIG = Report(_INIT_19AF_CONFIG_DATA[0], _INIT_19AF_CONFIG_DATA[1:])

D5NEXT_SAMPLE_STATUS_REPORT = bytes.fromhex(
    "00030DCB597C00010000006403FF00000051000004DC14000001E0007A98AF000"
    "00000FFFF000041A803C169000001481ACAA3465CB804B401F4000000527FFF7F"
    "FF7FFF7FFF7FFF7FFF7FFF7FFF000000000000000009D27FFF00007FFF01F404B"
    "400200026016D006300000004B200D7010207B80000000000098D083A098A083A"
    "00060001000000000000000000000000011A24015E27101D4CFFBF"
)

D5NEXT_SAMPLE_CONTROL_REPORT = bytes.fromhex(
    "00031E00000000000AC0007FFF0000000002020E100BB8000000000A0001000A0"
    "006000A000C000A0000000000000101F42710271007D000000027102710138802"
    "07D200000C8001F4012C00000064001E00010AF00A8C0AFD0B4C0B9D0BE90C460"
    "C9F0CF30D3C0DA20DE50E420E8A0EE60F350F7000000000000002D604D606D609"
    "810A010DAC1202162D17AD19D81EAE222E232E0212D300000D4801F4012C00000"
    "064001E00010AF00A8C0AFA0B4C0BA40C000C4F0CA30D110D510DA60DFD0E560E"
    "9E0EEE0F2010820000008C0000000000000000000001000180035407810A810B0"
    "10C810DD70EAC03E8FF000000000F030000FFFF0F19000003E80164000003E801"
    "FF0032006400000000000000000000000000000000000000000000FFFF0000FFF"
    "F0000FFFF0000FFFF0000FFFF0000FFFF000F0F080000FFFF0F19000003E80164"
    "000003E801FF00190028001400000000000000000000000000000000000000000"
    "00F03E7FFFF00FEFFFF0000FFFF0000FFFF0000FFFF001E0F0B0000FFFF0F1900"
    "0003E80164000003E801FF001E002800010006005000000000000000000000000"
    "0000002FF02FF01FBFFFF0525FFFF00C5FFFF03F5FFFF05F3FFFF002D0F040006"
    "FFFF0F19000003E80164000003E801FF002800050000000000000000000000000"
    "0000000000000000000000F0000FFFF01FDFFFF03FFFFFF00FAFFFF01CE10FF00"
    "3C0F040006FFFF0F19000003E80164000003E801FF00280005000000000000000"
    "00000000000000000000000000000000F00FAFFFF05DCFFFF01C2FFFF0000FFFF"
    "07D010FF004B0F040006FFFF0F19000003E80164000003E801FF0028000500000"
    "000000000000000000000000000000000000000000F03E8FFFF01C2FFFF0000FF"
    "FF0064FFFF032010FF010006030000FFFF0F19000003E80164000003E801FF001"
    "E006400000000000000000000000000000000000000000000FFFF0000FFFF0000"
    "FFFF0000FFFF0000FFFF0000FFFF010006000000FFFF0F19000003E8016400000"
    "3E80164001E006400000000000000000000000000000000000000000000FFFF00"
    "00FFFF0000FFFF0000FFFF0000FFFF0000FFFFC00401C20FA00110FB"
)

FARBWERK360_SAMPLE_STATUS_REPORT = bytes.fromhex(
    "000141BBDE9203E80000006403FE000000110000001A150000005F0008AE3E000"
    "00023BFC8C01AA20EFFD6A0E8A3915AEC0A3C0A470A6F09F87FFF7FFF7FFF7FFF"
    "7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF00000000000000000"
    "00000000000000001F901FA0006000000030000004300000000000A0324000000"
    "00000000002710271027102710271003E8000003E8000003E8000003E80000000"
    "0000000000000000000010002000101040006"
)

OCTO_SAMPLE_STATUS_REPORT = bytes.fromhex(
    "00023A92C9EA03E80001006503FB000000010000010DB4000000C5003C3EA4010"
    "00200000000000000000000000000059EDCFFDCFFDDFFDDA7A65BF80AC60ACF0B"
    "150D600EC87FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FF"
    "F7FFF0300000000000000000000000000000004B9000300030000055D04B90001"
    "00010000000008138804B9015E006702400000000000000000000000000000000"
    "00000000000000000000000000000000000000000000000000000000000000000"
    "0000000000000000000000000000000000000000213B04B900020002000000000"
    "80000000003E8055D0000000003E800000000000003E800000000000003E80000"
    "0000000003E800000000000003E800000000000003E800000000000003E8213B0"
    "000000003E827100000000003E827100000000000000000120412862710271098"
    "20"
)

OCTO_SAMPLE_CONTROL_REPORT = bytes.fromhex(
    "000228000000A90000051402BC000000000001F42710271007D00201F42710271"
    "007D00201F42710271007D00201F42710271007D00201F42710271007D00201F4"
    "2710271007D00201F42710271007D00001F42710271007D000055DFFFF0DAC057"
    "804B000000028001400010AF00A8C0AFA0B4A0BA40BF40C4E0C9D0CF80D480DA2"
    "0DF20E4C0E9C0EF50F460FA00000008C011801F4032004B0069008D40B680E4C1"
    "194152C19281D7422102710000000FFFF0DAC057804B000000028001400010AF0"
    "0A8C0AFA0B4A0BA40BF40C4E0C9D0CF80D480DA20DF20E4C0E9C0EF50F460FA00"
    "000008C011801F4032004B0069008D40B680E4C1194152C19281D742210271000"
    "0000FFFF0DAC057804B000000028001400010AF00A8C0AFA0B4A0BA40BF40C4E0"
    "C9D0CF80D480DA20DF20E4C0E9C0EF50F460FA00000008C011801F4032004B006"
    "9008D40B680E4C1194152C19281D7422102710000000FFFF0DAC057804B000000"
    "028001400010AF00A8C0AFA0B4A0BA40BF40C4E0C9D0CF80D480DA20DF20E4C0E"
    "9C0EF50F460FA00000008C011801F4032004B0069008D40B680E4C1194152C192"
    "81D7422102710000000FFFF0DAC057804B000000028001400010AF00A8C0AFA0B"
    "4A0BA40BF40C4E0C9D0CF80D480DA20DF20E4C0E9C0EF50F460FA00000008C011"
    "801F4032004B0069008D40B680E4C1194152C19281D7422102710000000FFFF0D"
    "AC057804B000000028001400010AF00A8C0AFA0B4A0BA40BF40C4E0C9D0CF80D4"
    "80DA20DF20E4C0E9C0EF50F460FA00000008C011801F4032004B0069008D40B68"
    "0E4C1194152C19281D7422102710000000FFFF0DAC057804B0000000280014000"
    "10AF00A8C0AFA0B4A0BA40BF40C4E0C9D0CF80D480DA20DF20E4C0E9C0EF50F46"
    "0FA00000008C011801F4032004B0069008D40B680E4C1194152C19281D7422102"
    "71000213BFFFF0DAC057804B000000028001400010AF00A8C0AFA0B4A0BA40BF4"
    "0C4E0C9D0CF80D480DA20DF20E4C0E9C0EF50F460FA00000008C011801F403200"
    "4B0069008D40B680E4C1194152C19281D74221027100000FF000000000F030000"
    "FFFF0F19000003E80164000003E801FF003200640000000000000000000000000"
    "0000000000000000000FFFF0000FFFF0000FFFF0000FFFF0000FFFF0000FFFF00"
    "0F0F080000FFFF0F19000003E80164000003E801FF00190028001400000000000"
    "00000000000000000000000000000000F03E7FFFF00FEFFFF0000FFFF0000FFFF"
    "0000FFFF001E0F0B0000FFFF0F19000003E80164000003E801FF001E002800010"
    "0060050000000000000000000000000000002FF02FF01FBFFFF0525FFFF00C5FF"
    "FF03F5FFFF05F3FFFF002D0F130000FFFF0F19000003E80164000003E801FF001"
    "9000A0005000500190000000000000000000000000000000000FF0200FF780000"
    "FFFF0000FFFF0000FFFF0000FFFF003C0F040006FFFF0F19000003E8016400000"
    "3E801FF0028000500000000000000000000000000000000000000000000000F00"
    "00FFFF01FDFFFF03FFFFFF00FAFFFF01CE10FF004B0F0F0000FFFF0F19000003E"
    "80164000003E801FF00280004001E001E00000000000000000000000000000000"
    "00000000007800780000FFFF0000FFFF0000FFFF0000FFFF01000F030000FFFF0"
    "F19000003E80164000003E801FF00320064000000000000000000000000000000"
    "00000000000000FFFF0000FFFF0000FFFF0000FFFF0000FFFF0000FFFF010F0F0"
    "80000FFFF0F19000003E80164000003E801FF0019002800140000000000000000"
    "000000000000000000000000000F03E7FFFF00FEFFFF0000FFFF0000FFFF0000F"
    "FFF011E0F0B0000FFFF0F19000003E80164000003E801FF001E00280001000600"
    "50000000000000000000000000000002FF02FF01FBFFFF0525FFFF00C5FFFF03F"
    "5FFFF05F3FFFF012D0F130000FFFF0F19000003E80164000003E801FF0019000A"
    "0005000500190000000000000000000000000000000000FF0200FF780000FFFF0"
    "000FFFF0000FFFF0000FFFF013C0F040006FFFF0F19000003E80164000003E801"
    "FF0028000500000000000000000000000000000000000000000000000F0000FFF"
    "F01FDFFFF03FFFFFF00FAFFFF01CE10FF014B0F0F0000FFFF0F19000003E80164"
    "000003E801FF00280004001E001E0000000000000000000000000000000000000"
    "000007800780000FFFF0000FFFF0000FFFF0000FFFF0100001388138813881388"
    "015E01AB59"
)

QUADRO_SAMPLE_STATUS_REPORT = bytes.fromhex(
    "00035B72FF4000010000006504080000000100000013C5000000910032CBB0000"
    "0000000000000FFD5FFD69B54FFD8A6FD5B977FFF7FFF06517FFF09597FFF7FFF"
    "7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF7FFF13887FFF7FFF7FFF0300000000000"
    "000000000000300000004B9000000000000000000000000000000271004B90000"
    "0000000000000805BB04B900000000016400000015E004B900000000000000000"
    "80000000003E800000000000003E827100000000003E805BB0000000003E815E0"
    "0000000003E82710000A0000000E000000002710FF000001"
)

QUADRO_SAMPLE_CONTROL_REPORT = bytes.fromhex(
    "00031C000000A9000002580514FAEC05DC0001F42710271007D00001F42710271"
    "007D00001F42710271007D00001F42710271007D0000000FFFF0DAC057804B000"
    "000028001400010AF00A8C0AFA0B4A0BA40BF40C4E0C9D0CF80D480DA20DF20E4"
    "C0E9C0EF50F460FA00000008C011801F4032004B0069008D40B680E4C1194152C"
    "19281D7422102710004CD0FFFF0DAC057804B000000028001400010AF00A8C0AF"
    "A0B4A0BA40BF40C4E0C9D0CF80D480DA20DF20E4C0E9C0EF50F460FA00000008C"
    "011801F4032004B0069008D40B680E4C1194152C19281D74221027100005BB000"
    "30DAC057804B000000028001400010AF00A8C0AFA0B4A0BA40BF40C4E0C9D0CF8"
    "0D480DA20DF20E4C0E9C0EF50F460FA00000008C011801F4032004B0069008D40"
    "B680E4C1194152C19281D74221027100015E0FFFF0DAC057804B0000000280014"
    "00010AF00A8C0AFA0B4A0BA40BF40C4E0C9D0CF80D480DA20DF20E4C0E9C0EF50"
    "F460FA00000008C011801F4032004B0069008D40B680E4C1194152C19281D7422"
    "102710FF000200000F030000FFFF0F19000003E80164000003E801FF003200640"
    "0000000000000000000000000000000000000000000FFFF0000FFFF0000FFFF00"
    "00FFFF0000FFFF0000FFFF000F0F080000FFFF0F19000003E80164000003E801F"
    "F0019002800140000000000000000000000000000000000000000000F03E7FFFF"
    "00FEFFFF0000FFFF0000FFFF0000FFFF001E0F0B0000FFFF0F19000003E801640"
    "00003E801FF001E0028000100060050000000000000000000000000000002FF02"
    "FF01FBFFFF0525FFFF00C5FFFF03F5FFFF05F3FFFF002D0F040006FFFF0F19000"
    "003E80164000003E801FF00280005000000000000000000000000000000000000"
    "00000000000F0000FFFF01FDFFFF03FFFFFF00FAFFFF01CE10FF003C0F040006F"
    "FFF0F19000003E80164000003E801FF0028000200000000000000000000000000"
    "000000000000000000000F03FFFFFF07D0FFFF0000FFFF0000FFFF0000FFFF004"
    "B0F040006FFFF0F19000003E80164000003E801FF002800020000000000000000"
    "0000000000000000000000000000000F01CEFFFF03FFFFFF0000FFFF0000FFFF0"
    "000FFFF002D0F000006FFFF0F19000003E80164000003E8016400280002000000"
    "00000000000000000000000000000000000000000F00FAFFFF01CE10FF0000FFF"
    "F0000FFFF0000FFFF002D0F000006FFFF0F19000003E80164000003E801640028"
    "000500000000000000000000000000000000000000000000000F0000FFFF01FDF"
    "FFF03FFFFFF00FAFFFF01CE10FF0100E0A8"
)

# fmt:off
krakenz3_response = {
    "liquid_hid": [[48, 1], [56, 1, 2, 0]],
    "brightness_hid": [[48, 1], [48, 2, 1, 60, 0, 0, 1, 0]],
    "orientation_hid": [[48, 1], [48, 2, 1, 50, 0, 0, 1, 1]],
    "static_hid": [
        [48, 1],
        [54, 3],
        [48, 4, 0],
        [48, 4, 1],
        [48, 4, 2],
        [48, 4, 3],
        [48, 4, 4],
        [48, 4, 5],
        [48, 4, 6],
        [48, 4, 7],
        [48, 4, 8],
        [48, 4, 9],
        [48, 4, 10],
        [48, 4, 11],
        [48, 4, 12],
        [48, 4, 13],
        [48, 4, 14],
        [48, 4, 15],
        [32, 3],
        [116, 1],
        [112, 1],
        [116, 1],
        [50, 2, 0],
        [50, 1, 0, 1, 0, 0, 144, 1, 1],
        [54, 1, 0],
        [54, 2],
        [56, 1, 4, 0],
    ],
    "gif_hid": [[48, 1],
                [54, 3],
                [48, 4, 0],
                [48, 4, 1],
                [48, 4, 2],
                [48, 4, 3],
                [48, 4, 4],
                [48, 4, 5],
                [48, 4, 6],
                [48, 4, 7],
                [48, 4, 8],
                [48, 4, 9],
                [48, 4, 10],
                [48, 4, 11],
                [48, 4, 12],
                [48, 4, 13],
                [48, 4, 14],
                [48, 4, 15],
                [32, 3],
                [116, 1],
                [112, 1],
                [116, 1],
                [50, 2, 0],
                [50, 1, 0, 1, 0, 0, 2, 0, 1],
                [54, 1, 0],
                [54, 2],
                [56, 1, 4, 0]],
    "static_bulk": [
        [18, 250, 1, 232, 171, 205, 239, 152, 118, 84, 50, 16, 2, 0, 0, 0, 0, 64, 6],
        [255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0, 255, 255, 1, 0,
         255, 255, 1, 0, 255, 255, 1, 0]],
    "gif_bulk": [
        [18, 250, 1, 232, 171, 205, 239, 152, 118, 84, 50, 16, 1, 0, 0, 0, 189, 6, 0],
        [71, 73, 70, 56, 57, 97, 64, 1, 64, 1, 129, 3, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 33, 255, 11, 78, 69, 84,
         83, 67, 65, 80, 69, 50, 46, 48, 3, 1, 0, 0, 0, 44, 0, 0, 0, 0, 64, 1, 64, 1, 0, 8, 255, 0, 1, 8, 28, 72, 176,
         160, 193, 131, 8, 19, 42, 92, 200, 176, 161, 195, 135, 16, 35, 74, 156, 72, 177, 162, 197, 139, 24, 51, 106,
         220, 200, 177, 163, 199, 143, 32, 67, 138, 28, 73, 178, 164, 201, 147, 40, 83, 170, 92, 201, 178, 165, 203,
         151, 48, 99, 202, 156, 73, 179, 166, 205, 155, 56, 115, 234, 220, 201, 179, 167, 207, 159, 64, 131, 10, 29, 74,
         180, 168, 209, 163, 72, 147, 42, 93, 202, 180, 169, 211, 167, 80, 163, 74, 157, 74, 181, 170, 213, 171, 88,
         179, 106, 221, 202, 181, 171, 215, 175, 96, 195, 138, 29, 75, 182, 172, 217, 179, 104, 211, 170, 93, 203, 182,
         173, 219, 183, 112, 227, 202, 157, 75, 183, 174, 221, 187, 120, 243, 234, 221, 203, 183, 175, 223, 191, 128, 3,
         11, 30, 76, 184, 176, 225, 195, 136, 19, 43, 94, 204, 184, 177, 227, 199, 144, 35, 75, 158, 76, 185, 178, 229,
         203, 152, 51, 107, 222, 204, 185, 179, 231, 207, 160, 67, 139, 30, 77, 186, 180, 233, 211, 168, 83, 171, 94,
         205, 186, 181, 235, 215, 176, 99, 203, 158, 77, 187, 182, 237, 219, 184, 115, 235, 222, 205, 187, 183, 239,
         223, 192, 131, 11, 31, 78, 188, 184, 241, 227, 200, 147, 43, 95, 206, 188, 185, 243, 231, 208, 163, 75, 159,
         78, 189, 186, 245, 235, 216, 179, 107, 223, 206, 189, 187, 247, 239, 224, 195, 139, 255, 31, 79, 190, 188, 249,
         243, 232, 211, 171, 95, 207, 190, 189, 251, 247, 240, 227, 203, 159, 79, 191, 190, 253, 251, 248, 243, 235,
         223, 207, 191, 191, 255, 255, 0, 6, 40, 224, 128, 4, 22, 104, 224, 129, 8, 38, 168, 224, 130, 12, 54, 232, 224,
         131, 16, 70, 40, 225, 132, 20, 86, 104, 225, 133, 24, 102, 168, 225, 134, 28, 118, 232, 225, 135, 32, 134, 40,
         226, 136, 36, 150, 104, 226, 137, 40, 166, 168, 226, 138, 44, 182, 232, 226, 139, 48, 198, 40, 227, 140, 52,
         214, 104, 227, 141, 56, 230, 168, 227, 142, 60, 246, 232, 227, 143, 64, 6, 41, 228, 144, 68, 22, 105, 228, 145,
         72, 38, 169, 228, 146, 76, 54, 233, 228, 147, 80, 70, 41, 229, 148, 84, 86, 105, 229, 149, 88, 102, 169, 229,
         150, 92, 118, 233, 229, 151, 96, 134, 41, 230, 152, 100, 150, 105, 230, 153, 104, 166, 169, 230, 154, 108, 182,
         233, 230, 155, 112, 198, 41, 231, 156, 116, 214, 105, 231, 157, 120, 230, 169, 231, 158, 124, 246, 233, 231,
         159, 128, 6, 42, 232, 160, 132, 22],
        [106, 232, 161, 136, 38, 170, 232, 162, 140, 54, 234, 232, 163, 144, 70, 42, 233, 164, 148, 86, 106, 233, 165,
         152, 102, 170, 233, 166, 156, 118, 234, 233, 167, 160, 134, 42, 234, 168, 164, 150, 106, 234, 169, 168, 166,
         170, 234, 170, 172, 182, 234, 234, 171, 176, 198, 27, 42, 235, 172, 180, 214, 106, 235, 173, 184, 230, 170,
         235, 174, 188, 246, 234, 235, 175, 192, 6, 43, 236, 176, 196, 90, 20, 16, 0, 44, 0, 0, 0, 0, 64, 1, 64, 1, 129,
         0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 255, 0, 1, 8, 28, 72, 176, 160, 193, 131, 8, 19, 42, 92, 200, 176,
         161, 195, 135, 16, 35, 74, 156, 72, 177, 162, 197, 139, 24, 51, 106, 220, 200, 177, 163, 199, 143, 32, 67, 138,
         28, 73, 178, 164, 201, 147, 40, 83, 170, 92, 201, 178, 165, 203, 151, 48, 99, 202, 156, 73, 179, 166, 205, 155,
         56, 115, 234, 220, 201, 179, 167, 207, 159, 64, 131, 10, 29, 74, 180, 168, 209, 163, 72, 147, 42, 93, 202, 180,
         169, 211, 167, 80, 163, 74, 157, 74, 181, 170, 213, 171, 88, 179, 106, 221, 202, 181, 171, 215, 175, 96, 195,
         138, 29, 75, 182, 172, 217, 179, 104, 211, 170, 93, 203, 182, 173, 219, 183, 112, 227, 202, 157, 75, 183, 174,
         221, 187, 120, 243, 234, 221, 203, 183, 175, 223, 191, 128, 3, 11, 30, 76, 184, 176, 225, 195, 136, 19, 43, 94,
         204, 184, 177, 227, 199, 144, 35, 75, 158, 76, 185, 178, 229, 203, 152, 51, 107, 222, 204, 185, 179, 231, 207,
         160, 67, 139, 30, 77, 186, 180, 233, 211, 168, 83, 171, 94, 205, 186, 181, 235, 215, 176, 99, 203, 158, 77,
         187, 182, 237, 219, 184, 115, 235, 222, 205, 187, 183, 239, 223, 192, 131, 11, 31, 78, 188, 184, 241, 227, 200,
         147, 43, 95, 206, 188, 185, 243, 231, 208, 163, 75, 159, 78, 189, 186, 245, 235, 216, 179, 107, 223, 206, 189,
         187, 247, 239, 224, 195, 139, 255, 31, 79, 190, 188, 249, 243, 232, 211, 171, 95, 207, 190, 189, 251, 247, 240,
         227, 203, 159, 79, 191, 190, 253, 251, 248, 243, 235, 223, 207, 191, 191, 255, 255, 0, 6, 40, 224, 128, 4, 22,
         104, 224, 129, 8, 38, 168, 224, 130, 12, 54, 232, 224, 131, 16, 70, 40, 225, 132, 20, 86, 104, 225, 133, 24,
         102, 168, 225, 134, 28, 118, 232, 225, 135, 32, 134, 40, 226, 136, 36, 150, 104, 226, 137, 40, 166, 168, 226,
         138, 44, 182, 232, 226, 139, 48, 198, 40, 227, 140, 52, 214, 104, 227, 141, 56, 230, 168, 227, 142, 60, 246,
         232, 227, 143, 64, 6, 41, 228, 144, 68, 22, 105, 228, 145, 72, 38, 169, 228, 146, 76, 54, 233, 228, 147, 80,
         70, 41, 229, 148, 84, 86, 105, 229, 149, 88, 102, 169, 229, 150],
        [92, 118, 233, 229, 151, 96, 134, 41, 230, 152, 100, 150, 105, 230, 153, 104, 166, 169, 230, 154, 108, 182, 233,
         230, 155, 112, 198, 41, 231, 156, 116, 214, 105, 231, 157, 120, 230, 169, 231, 158, 124, 246, 233, 231, 159,
         128, 6, 42, 232, 160, 132, 22, 106, 232, 161, 136, 38, 170, 232, 162, 140, 54, 234, 232, 163, 144, 70, 42, 233,
         164, 148, 86, 106, 233, 165, 152, 102, 170, 233, 166, 156, 118, 234, 233, 167, 160, 134, 42, 234, 168, 164,
         150, 106, 234, 169, 168, 166, 170, 234, 170, 172, 182, 234, 234, 171, 176, 198, 27, 42, 235, 172, 180, 214,
         106, 235, 173, 184, 230, 170, 235, 174, 188, 246, 234, 235, 175, 192, 6, 43, 236, 176, 196, 90, 20, 16, 0, 44,
         0, 0, 0, 0, 64, 1, 64, 1, 129, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 255, 0, 1, 8, 28, 72, 176, 160, 193,
         131, 8, 19, 42, 92, 200, 176, 161, 195, 135, 16, 35, 74, 156, 72, 177, 162, 197, 139, 24, 51, 106, 220, 200,
         177, 163, 199, 143, 32, 67, 138, 28, 73, 178, 164, 201, 147, 40, 83, 170, 92, 201, 178, 165, 203, 151, 48, 99,
         202, 156, 73, 179, 166, 205, 155, 56, 115, 234, 220, 201, 179, 167, 207, 159, 64, 131, 10, 29, 74, 180, 168,
         209, 163, 72, 147, 42, 93, 202, 180, 169, 211, 167, 80, 163, 74, 157, 74, 181, 170, 213, 171, 88, 179, 106,
         221, 202, 181, 171, 215, 175, 96, 195, 138, 29, 75, 182, 172, 217, 179, 104, 211, 170, 93, 203, 182, 173, 219,
         183, 112, 227, 202, 157, 75, 183, 174, 221, 187, 120, 243, 234, 221, 203, 183, 175, 223, 191, 128, 3, 11, 30,
         76, 184, 176, 225, 195, 136, 19, 43, 94, 204, 184, 177, 227, 199, 144, 35, 75, 158, 76, 185, 178, 229, 203,
         152, 51, 107, 222, 204, 185, 179, 231, 207, 160, 67, 139, 30, 77, 186, 180, 233, 211, 168, 83, 171, 94, 205,
         186, 181, 235, 215, 176, 99, 203, 158, 77, 187, 182, 237, 219, 184, 115, 235, 222, 205, 187, 183, 239, 223,
         192, 131, 11, 31, 78, 188, 184, 241, 227, 200, 147, 43, 95, 206, 188, 185, 243, 231, 208, 163, 75, 159, 78,
         189, 186, 245, 235, 216, 179, 107, 223, 206, 189, 187, 247, 239, 224, 195, 139, 255, 31, 79, 190, 188, 249,
         243, 232, 211, 171, 95, 207, 190, 189, 251, 247, 240, 227, 203, 159, 79, 191, 190, 253, 251, 248, 243, 235,
         223, 207, 191, 191, 255, 255, 0, 6, 40, 224, 128, 4, 22, 104, 224, 129, 8, 38, 168, 224, 130, 12, 54, 232, 224,
         131, 16, 70, 40, 225, 132, 20, 86, 104, 225, 133, 24, 102, 168, 225, 134, 28, 118, 232, 225, 135, 32, 134, 40,
         226, 136, 36, 150, 104, 226, 137, 40, 166, 168, 226, 138, 44, 182, 232, 226, 139, 48, 198, 40],
        [227, 140, 52, 214, 104, 227, 141, 56, 230, 168, 227, 142, 60, 246, 232, 227, 143, 64, 6, 41, 228, 144, 68, 22,
         105, 228, 145, 72, 38, 169, 228, 146, 76, 54, 233, 228, 147, 80, 70, 41, 229, 148, 84, 86, 105, 229, 149, 88,
         102, 169, 229, 150, 92, 118, 233, 229, 151, 96, 134, 41, 230, 152, 100, 150, 105, 230, 153, 104, 166, 169, 230,
         154, 108, 182, 233, 230, 155, 112, 198, 41, 231, 156, 116, 214, 105, 231, 157, 120, 230, 169, 231, 158, 124,
         246, 233, 231, 159, 128, 6, 42, 232, 160, 132, 22, 106, 232, 161, 136, 38, 170, 232, 162, 140, 54, 234, 232,
         163, 144, 70, 42, 233, 164, 148, 86, 106, 233, 165, 152, 102, 170, 233, 166, 156, 118, 234, 233, 167, 160, 134,
         42, 234, 168, 164, 150, 106, 234, 169, 168, 166, 170, 234, 170, 172, 182, 234, 234, 171, 176, 198, 27, 42, 235,
         172, 180, 214, 106, 235, 173, 184, 230, 170, 235, 174, 188, 246, 234, 235, 175, 192, 6, 43, 236, 176, 196, 90,
         20, 16, 0, 59]]
}


# fmt:on


class TestMocks:
    """Test Mock Instance Factory"""

    ####################################################################################################################
    # Kraken 2

    @staticmethod
    def mockKrakenX2Device() -> Kraken2:
        device = _MockKraken2Device(fw_version=(6, 0, 2))
        return Kraken2(
            device,
            "NZXT Kraken X (X42, X52, X62 or X72)",
            device_type=Kraken2.DEVICE_KRAKENX,
        )

    @staticmethod
    def mockKrakenM2Device() -> Kraken2:
        device = _MockKraken2Device(fw_version=(6, 0, 2))
        return Kraken2(device, "NZXT Kraken M22", device_type=Kraken2.DEVICE_KRAKENM)

    ####################################################################################################################
    # Kraken 3

    @staticmethod
    def mockKrakenX3Device() -> KrakenX3:
        raw = MockKraken(raw_led_channels=len(_COLOR_CHANNELS_KRAKENX) - 1)
        return KrakenX3(
            raw,
            "NZXT Kraken X (X53, X63 or X73)",
            speed_channels=_SPEED_CHANNELS_KRAKENX,
            color_channels=_COLOR_CHANNELS_KRAKENX,
        )

    @staticmethod
    def mockKrakenZ3Device() -> KrakenZ3:
        raw = MockKraken(raw_led_channels=1)
        return MockKrakenZ3(
            raw,
            "Kraken Z73",
            speed_channels=_SPEED_CHANNELS_KRAKENZ,
            color_channels=_COLOR_CHANNELS_KRAKENZ,
        )

    ####################################################################################################################
    # Corsair Commander Pro

    @staticmethod
    def mockCommanderProDevice() -> CommanderPro:
        device = MockHidapiDevice(vendor_id=0x1B1C, product_id=0x0C10, address="addr")
        pro = CommanderPro(device, "Corsair Commander Pro", 6, 4, 2)
        runtime_storage = MockRuntimeStorage(key_prefixes=["testing"])
        pro.connect(runtime_storage=runtime_storage)
        return pro

    ####################################################################################################################
    # NZXT H1 V2

    @staticmethod
    def mockH1V2() -> H1V2:
        device = MockH1V2(raw_speed_channels=2, raw_led_channels=0)
        return H1V2(device, "NZXT H1 V2", speed_channel_count=2, color_channel_count=0)

    ####################################################################################################################
    # NZXT Smart Device V2

    @staticmethod
    def mockSmartDevice2() -> SmartDevice2:
        device = _MockSmartDevice2(raw_speed_channels=3, raw_led_channels=2)
        return SmartDevice2(
            device, "NZXT Smart Device V2", speed_channel_count=3, color_channel_count=2
        )

    ####################################################################################################################
    # NZXT Smart Device V1

    @staticmethod
    def mockSmartDevice() -> SmartDevice:
        device = MockHidapiDevice(vendor_id=0x1E71, product_id=0x1714, address="addr")
        return SmartDevice(
            device, "NZXT Smart Device V1", speed_channel_count=3, color_channel_count=1
        )

    ####################################################################################################################
    # Modern Asetek:
    # (EVGA CLC 120 (CLC12), 240, 280 and 360,
    # Corsair Hydro H80i v2, H100i v2 and H115i,
    # Corsair Hydro H80i GT, H100i GTX and H110i GTX)

    @staticmethod
    def mockModern690LcDevice() -> Modern690Lc:
        device = MockPyusbDevice()
        return Modern690Lc(device, "Modern 690LC - EVGA, Corsair")

    ####################################################################################################################
    # Legacy Asetek: (NZXT Kraken X40, X60, X31, X41, X51 and X61)

    @staticmethod
    def mockLegacy690LcDevice() -> Modern690Lc:
        device = MockPyusbDevice(vendor_id=0xFFFF, product_id=0xB200, bus=1, port=(1,))
        # return Legacy690Lc(device, 'NZXT Kraken X60')
        # the legacy devices are detected as moderns at first and user has to select if this is a legacy or not.
        return Modern690Lc(device, "NZXT Kraken X60")

    ####################################################################################################################
    # ITE 8297: found in Gigabyte X570 Aorus Elite - RGB Fusion

    @staticmethod
    def mockRgbFusion2_8297Device() -> RgbFusion2:
        device = Mock8297HidInterface(
            vendor_id=0x048D, product_id=0x8297, address="addr"
        )
        return RgbFusion2(device, "Gigabyte RGB Fusion 2.0 8297 Controller")

    ####################################################################################################################
    # Corsair HID PSU

    @staticmethod
    def mock_corsair_psu() -> CorsairHidPsu:
        kwargs = {
            "fpowin115": (
                0.00013153276902318052,
                1.0118732314945875,
                9.783796618886313,
            ),
            "fpowin230": (9.268856467314546e-05, 1.0183515407387007, 8.279822175342481),
        }
        device = MockCorsairPsu(vendor_id=0x1B1C, product_id=0x1C05, address="addr")
        return CorsairHidPsu(device, "Corsair HX750i", **kwargs)

    ####################################################################################################################
    # NZXT E PSU

    @staticmethod
    def mockNzxtPsuDevice() -> NzxtEPsu:
        device = _MockNzxtPsuDevice()
        return NzxtEPsu(device, "NZXT E500 PSU")

    ####################################################################################################################
    # AseTek Pro

    @staticmethod
    def mockHydroPro() -> HydroPro:
        usb_dev = MockPyusbDevice()
        return HydroPro(usb_dev, "Asetek Pro cooler", fan_count=2)

    ####################################################################################################################
    # Hydro Platinum - choose the model with the most features to mock

    @staticmethod
    def mockHydroPlatinumSeDevice() -> HydroPlatinum:
        description = "H115i Platinum"
        kwargs = {"fan_count": 2, "fan_leds": 4}
        device = _MockHydroPlatinumDevice()
        return HydroPlatinum(device, description, **kwargs)

    ####################################################################################################################
    # Corsair Commander Core & Corsair iCUE

    @staticmethod
    def mock_commander_core_device() -> CommanderCore:
        device = MockCommanderCoreDevice()
        return CommanderCore(device, "Corsair Commander Core (experimental)", True)

    ####################################################################################################################
    # ASUS Aura LED Controller

    @staticmethod
    def mockAuraLed_19AFDevice() -> AuraLed:
        device = MockHidapiDevice(vendor_id=0x0B05, product_id=0x19AF, address="addr")
        return AuraLed(device, "Aura LED Controller")

    ####################################################################################################################
    # Aquacomputer D5 Next

    @staticmethod
    def mockAquacomputer_d5NextDevice() -> Aquacomputer:
        device = _MockD5NextDevice()
        return Aquacomputer(
            device,
            "Aquacomputer D5 Next",
            device_info=Aquacomputer._DEVICE_INFO[Aquacomputer._DEVICE_D5NEXT],
        )

    ####################################################################################################################
    # Aquacomputer Farbwerk360

    @staticmethod
    def mockAquacomputer_Farbwerk360Device() -> Aquacomputer:
        device = _MockFarbwerk360Device()
        return Aquacomputer(
            device,
            "Aquacomputer Farbwerk 360",
            device_info=Aquacomputer._DEVICE_INFO[Aquacomputer._DEVICE_FARBWERK360],
        )

    ####################################################################################################################
    # Aquacomputer Octo

    @staticmethod
    def mockAquacomputer_OctoDevice() -> Aquacomputer:
        device = _MockOctoDevice()
        return Aquacomputer(
            device,
            "Aquacomputer Octo",
            device_info=Aquacomputer._DEVICE_INFO[Aquacomputer._DEVICE_OCTO],
        )

    ####################################################################################################################
    # Aquacomputer Quadro

    @staticmethod
    def mockAquacomputer_QuadroDevice() -> Aquacomputer:
        device = _MockQuadroDevice()
        return Aquacomputer(
            device,
            "Aquacomputer Quadro",
            device_info=Aquacomputer._DEVICE_INFO[Aquacomputer._DEVICE_QUADRO],
        )


########################################################################################################################
# Mock Class Helpers:


class _MockKraken2Device(MockHidapiDevice):
    def __init__(self, fw_version):
        super().__init__(vendor_id=0xFFFF, product_id=0x1E71)
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
        buf[3:5] = self.fan_speed.to_bytes(length=2, byteorder="big")
        buf[5:7] = self.pump_speed.to_bytes(length=2, byteorder="big")
        major, minor, patch = self.fw_version
        buf[0xB] = major
        buf[0xC:0xE] = minor.to_bytes(length=2, byteorder="big")
        buf[0xE] = patch
        return buf[:length]


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


class MockH1V2(MockHidapiDevice):
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
        return super().write(data)


class Mock8297HidInterface(MockHidapiDevice):
    def get_feature_report(self, report_id, length):
        """Get a feature report emulating out of spec behavior of the device."""
        return super().get_feature_report(0, length)


class MockCorsairPsu(MockHidapiDevice):
    def __init__(self, *args, **kwargs):
        self._page = 0
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
            cmd = f"{data[1]:02x}"
            samples = [
                x for x in CORSAIR_SAMPLE_PAGED_RESPONSES[self._page] if x[2:4] == cmd
            ]
            if not samples:
                samples = [x for x in CORSAIR_SAMPLE_RESPONSES if x[2:4] == cmd]
            if not samples:
                raise KeyError(cmd)
            reply[0 : len(data)] = bytes.fromhex(samples[0])
            self.preload_read(Report(0, reply))


class _MockNzxtPsuDevice(MockHidapiDevice):
    def write(self, data):
        super().write(data)
        data = data[1:]  # skip unused report ID
        reply = bytearray(64)
        reply[0:2] = (0xAA, data[2])
        if data[5] == 0x06:
            reply[2] = data[2] - 2
        elif data[5] == 0xFC:
            reply[2:4] = (0x11, 0x41)
        self.preload_read(Report(0, reply[0:]))


class _MockHydroPlatinumDevice(MockHidapiDevice):
    def __init__(self):
        super().__init__(
            vendor_id=0xFFFF, product_id=0x0C17, address=HYDRO_PLATINUM_SAMPLE_PATH
        )
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
        buf[14] = round(0.10 * 255)
        buf[15:17] = self.fan1_speed.to_bytes(length=2, byteorder="little")
        buf[21] = round(0.20 * 255)
        buf[22:24] = self.fan2_speed.to_bytes(length=2, byteorder="little")
        buf[28] = round(0.70 * 255)
        buf[29:31] = self.pump_speed.to_bytes(length=2, byteorder="little")
        buf[42] = round(0.30 * 255)
        buf[43:44] = self.fan3_speed.to_bytes(length=2, byteorder="little")
        buf[-1] = compute_pec(buf[1:-1])
        return buf[:length]


def int_to_le(num, length=2, byteorder="little", signed=False):
    """Helper method for the MockCommanderCoreDevice"""
    return int(num).to_bytes(length=length, byteorder=byteorder, signed=signed)


class MockCommanderCoreDevice:
    def __init__(self):
        self.vendor_id = 0x1B1C
        self.product_id = 0x0C1C
        self.address = "addr"
        self.path = b"path"
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
                mode = self._modes.get(channel)
                if mode[1] == 0x00:
                    if mode[0] == 0x17:  # Get speeds
                        data.extend([0x06, 0x00])
                        data.append(len(self.speeds))
                        for i in self.speeds:
                            if i is None:
                                data.extend([0x00, 0x00])
                            else:
                                data.extend(int_to_le(i))
                    elif mode[0] == 0x1A:  # Speed devices connected
                        data.extend([0x09, 0x00])
                        data.append(len(self.speeds))
                        for i in self.speeds:
                            data.extend([0x01 if i is None else 0x07])
                    elif mode[0] == 0x20:  # LED detect
                        data.extend([0x0F, 0x00])
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
                elif mode[1] == 0x6D:
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
            raise ValueError("Start of packets going out should be 00:08")
        if data[2] == 0x0D:
            channel = data[3]
            if self._modes.get(channel) is None:
                self._modes[channel] = data[4:6]
            else:
                raise ExpectationNotMet("Previous channel was not reset")
        elif data[2] == 0x05 and data[3] == 0x01:
            self._modes[data[4]] = None
        elif data[2] == 0x01 and data[3] == 0x03 and data[4] == 0x00:
            self._awake = data[5] == 0x02
        elif self._awake:
            if data[2] == 0x06:  # Write command
                channel = data[3]
                mode = self._modes.get(channel)
                length = u16le_from(data[4:6])
                data_type = data[8:10]
                written_data = data[10 : 8 + length]
                if mode[1] == 0x6D:
                    if mode[0] == 0x60 and list(data_type) == [0x03, 0x00]:
                        self.speeds_mode = tuple(
                            written_data[i + 1] for i in range(0, written_data[0])
                        )
                    elif mode[0] == 0x61 and list(data_type) == [0x04, 0x00]:
                        self.fixed_speeds = tuple(
                            u16le_from(written_data[i * 2 + 1 : i * 2 + 3])
                            for i in range(0, written_data[0])
                        )
                    else:
                        raise NotImplementedError("Invalid Write command")
                else:
                    raise NotImplementedError("Invalid Write command")

        return len(data)


class _MockD5NextDevice(MockHidapiDevice):
    def __init__(self):
        super().__init__(vendor_id=0x0C70, product_id=0xF00E)

        self.preload_read(Report(1, D5NEXT_SAMPLE_STATUS_REPORT))
        # currently not needed for our use case, just the status report:
        # self.preload_read(Report(3, D5NEXT_SAMPLE_CONTROL_REPORT))
        # self.preload_read(Report(3, D5NEXT_SAMPLE_CONTROL_REPORT))

    def read(self, length):
        pre = super().read(length)
        if pre:
            return pre

        return Report(1, D5NEXT_SAMPLE_STATUS_REPORT)


class _MockFarbwerk360Device(MockHidapiDevice):
    def __init__(self):
        super().__init__(vendor_id=0x0C70, product_id=0xF010)

        self.preload_read(Report(1, FARBWERK360_SAMPLE_STATUS_REPORT))

    def read(self, length):
        pre = super().read(length)
        if pre:
            return pre

        return Report(1, FARBWERK360_SAMPLE_STATUS_REPORT)


class _MockOctoDevice(MockHidapiDevice):
    def __init__(self):
        super().__init__(vendor_id=0x0C70, product_id=0xF011)

        self.preload_read(Report(1, OCTO_SAMPLE_STATUS_REPORT))
        # currently not needed for our use case, just the status report:
        # self.preload_read(Report(3, OCTO_SAMPLE_CONTROL_REPORT))
        # self.preload_read(Report(3, OCTO_SAMPLE_CONTROL_REPORT))

    def read(self, length):
        pre = super().read(length)
        if pre:
            return pre

        return Report(1, OCTO_SAMPLE_STATUS_REPORT)


class _MockQuadroDevice(MockHidapiDevice):
    def __init__(self):
        super().__init__(vendor_id=0x0C70, product_id=0xF00D)

        self.preload_read(Report(1, QUADRO_SAMPLE_STATUS_REPORT))
        # currently not needed for our use case, just the status report:
        # self.preload_read(Report(3, QUADRO_SAMPLE_CONTROL_REPORT))
        # self.preload_read(Report(3, QUADRO_SAMPLE_CONTROL_REPORT))

    def read(self, length):
        pre = super().read(length)
        if pre:
            return pre

        return Report(1, QUADRO_SAMPLE_STATUS_REPORT)


class MockKraken(MockHidapiDevice):
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
        elif data[0:2] == [0x30, 0x01]:
            reply[0:2] = [0x31, 0x01]
            reply[0x18] = 50  # lcd brightness
            reply[0x1A] = 0  # lcd orientation
        elif data[0:2] == [0x32, 0x1]:  # setup bucket
            reply[14] = 0x1
        elif data[0:2] == [0x32, 0x2]:  # delete bucker
            reply[0:2] = [0x33, 0x02]
            reply[14] = 0x1
        elif data[0:2] == [0x38, 0x1]:  # switch bucket
            reply[14] = 0x1

        self.preload_read(Report(0, reply))
        return super().write(data)


if E2E_TESTING_ENABLED:

    class MockKrakenZ3(KrakenZ3):
        def __init__(
            self, device, description, speed_channels, color_channels, **kwargs
        ):
            KrakenX3.__init__(
                self,
                device,
                description,
                speed_channels,
                color_channels,
                _HWMON_CTRL_MAPPING_KRAKENZ,
                **kwargs,
            )

            self.bulk_device = MockPyusbDevice(0x1E71, 0x3008)
            self.bulk_device.close_winusb_device = self.bulk_device.release

            self.orientation = 0
            self.brightness = 50

            self.screen_mode = None

        def set_screen(self, channel, mode, value, **kwargs):
            self.screen_mode = mode
            self.hid_data_index = 0
            self.bulk_data_index = 0

            super().set_screen(channel, mode, value, **kwargs)

            assert self.hid_data_index == len(
                krakenz3_response[self.screen_mode + "_hid"]
            ), f"Incorrect number of hid messages sent for mode: {mode}"

            if mode == "static" or mode == "gif":
                assert (
                    self.bulk_data_index == 801
                    if mode == "static"
                    else len(krakenz3_response[self.screen_mode + "_bulk"])
                ), f"Incorrect number of bulk messages sent for mode: {mode}"

        def _write(self, data):
            if self.screen_mode:
                # this assert causes a read error on every get_status() after setting the screen:
                # assert (
                #         data == krakenz3_response[self.screen_mode + "_hid"][self.hid_data_index]
                # ), f"HID write failed, wrong data for mode: {self.screen_mode}, data index: {self.hid_data_index}"
                self.hid_data_index += 1
            return super()._write(data)

        def _bulk_write(self, data):
            fixed_data_index = self.bulk_data_index
            if (
                self.screen_mode == "static" and self.bulk_data_index > 1
            ):  # the rest of the message should be identical to index 1
                fixed_data_index = 1

            # this assert causes a read error on every get_status() after setting the screen:
            # assert (
            #         data == krakenz3_response[self.screen_mode + "_bulk"][fixed_data_index]
            # ), f"Bulk write failed, wrong data for mode: {self.screen_mode}, data index: {self.bulk_data_index}"
            self.bulk_data_index += 1
            return super()._bulk_write(data)
