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
import re
from re import Pattern
from typing import Optional, List, Dict, Tuple, Union, Any

import liquidctl
import matplotlib
import numpy
from PIL import Image
from liquidctl.driver.asetek import Modern690Lc, Legacy690Lc, Hydro690Lc
from liquidctl.driver.asetek_pro import HydroPro
from liquidctl.driver.aura_led import AuraLed
from liquidctl.driver.base import BaseDriver
from liquidctl.driver.corsair_hid_psu import CorsairHidPsu
from liquidctl.driver.hydro_platinum import HydroPlatinum
from liquidctl.driver.kraken2 import Kraken2

from coolero.dialogs.legacy_690_dialog import Legacy690Dialog
from coolero.dialogs.legacy_kraken2_firmware_dialog import LegacyKraken2FirmwareDialog
from coolero.exceptions.device_communication_error import DeviceCommunicationError
from coolero.models.device import Device, DeviceType
from coolero.models.device_info import DeviceInfo
from coolero.models.settings import Setting
from coolero.models.status import Status
from coolero.repositories.devices_repository import DevicesRepository
from coolero.services.device_extractor import DeviceExtractor
from coolero.settings import Settings as AppSettings, FeatureToggle, UserSettings

_LOG = logging.getLogger(__name__)
_LC_CALLED: str = "liquidctl request:"
_LC_RESPONSE: str = "liquidctl response:"


class LiquidctlRepo(DevicesRepository):
    """Repo for all Liquidctl devices"""

    _devices_drivers: Dict[int, Tuple[Device, BaseDriver]] = {}
    _device_info_extractor: DeviceExtractor
    _pattern_number: Pattern = re.compile(r'\d+$')

    def __init__(self) -> None:
        self._device_info_extractor = DeviceExtractor()
        super().__init__()
        _LOG.info("Successfully initialized")
        _LOG.debug("All liquidctl devices initialized with status: %s", self._devices_drivers)

    @property
    def statuses(self) -> List[Device]:
        return [device for device, _ in self._devices_drivers.values()]

    def update_statuses(self) -> None:
        for lc_device_id, (device, lc_device) in self._devices_drivers.items():
            if FeatureToggle.testing:
                from coolero.repositories.test_repo_ext import TestRepoExtension
                TestRepoExtension.prepare_for_mocks_get_status(device, lc_device)
            _LOG.debug_lc("%s %s.get_status()", _LC_CALLED, lc_device.__class__.__name__)
            lc_status: List[Tuple] = lc_device.get_status()
            _LOG.debug_lc("%s %s", _LC_RESPONSE, lc_status)
            device.status = self._map_status(lc_device, lc_status, lc_device_id)
            _LOG.debug("Liquidctl device: %s status was updated with: %s", device.name, device.status)

    def shutdown(self) -> None:
        """Should be run on exit & shutdown, even in case of exception"""
        self._handle_device_specific_shutdowns()
        for _, lc_device in self._devices_drivers.values():
            _LOG.debug_lc("%s %s.disconnect()", _LC_CALLED, lc_device.__class__.__name__)
            lc_device.disconnect()
        self._devices_drivers.clear()
        _LOG.debug("Liquidctl Repo shutdown")

    def set_settings(self, lc_device_id: int, setting: Setting) -> Optional[str]:
        device, lc_device = self._devices_drivers[lc_device_id]
        try:
            if setting.speed_fixed is not None:
                self._set_speed_fixed(setting, device, lc_device)
            elif setting.speed_profile:
                self._set_speed_profile(setting, lc_device)
            elif setting.lighting is not None:
                self._set_color(setting, device, lc_device)
            elif setting.lcd is not None:
                self._set_screen(setting, lc_device)
            return device.name
        except BaseException as ex:
            _LOG.error('An Error has occurred when trying to set the settings: %s', ex)
            return None

    @staticmethod
    def _set_speed_fixed(setting: Setting, device: Device, lc_device: BaseDriver) -> None:
        kwargs: Dict[str, Any] = {}
        if device.lc_driver_type == HydroPlatinum and setting.channel_name == 'pump':
            # limits from tested Hydro H150i Pro XT
            if setting.speed_fixed < 56:
                kwargs['pump_mode'] = 'quiet'
            elif setting.speed_fixed > 75:
                kwargs['pump_mode'] = 'extreme'
            else:
                kwargs['pump_mode'] = 'balanced'  # default setting
            _LOG.debug_lc("%s %s.initialize(%s)", _LC_CALLED, lc_device.__class__.__name__, kwargs)
            lc_device.initialize(**kwargs)
        elif device.lc_driver_type == HydroPro and setting.channel_name == 'pump':
            if setting.speed_fixed < 34:
                kwargs['pump_mode'] = 'quiet'
            elif setting.speed_fixed > 66:
                kwargs['pump_mode'] = 'performance'
            else:
                kwargs['pump_mode'] = 'balanced'
            _LOG.debug_lc("%s %s.initialize(%s)", _LC_CALLED, lc_device.__class__.__name__, kwargs)
            lc_device.initialize(**kwargs)
        else:
            kwargs['channel'] = setting.channel_name
            kwargs['duty'] = setting.speed_fixed
            # This is used as we aren't running liquidctl as root for setting values per hwmon.
            #  Currently only applies to aquacomputer driver, but coming for other drivers soon.
            kwargs['direct_access'] = True
            _LOG.debug_lc("%s %s.set_fixed_speed(%s)", _LC_CALLED, lc_device.__class__.__name__, kwargs)
            lc_device.set_fixed_speed(**kwargs)

    def _set_speed_profile(self, setting: Setting, lc_device: BaseDriver) -> None:
        matched_sensor_number = self._pattern_number.search(setting.temp_source.name)
        temp_sensor_number = int(matched_sensor_number.group()) if matched_sensor_number else None
        kwargs: Dict[str, Any] = {
            'channel': setting.channel_name,
            'profile': setting.speed_profile,
            'temperature_sensor': temp_sensor_number,
        }
        _LOG.debug_lc("%s %s.set_speed_profile(%s)", _LC_CALLED, lc_device.__class__.__name__, kwargs)
        lc_device.set_speed_profile(**kwargs)

    @staticmethod
    def _set_color(setting: Setting, device: Device, lc_device: BaseDriver) -> None:
        kwargs: Dict[str, Any] = {
            'channel': setting.channel_name,
            'mode': setting.lighting.mode,
            'colors': setting.lighting.colors,
        }
        if setting.lighting.speed is not None:
            if device.lc_driver_type == Legacy690Lc:
                kwargs['time_per_color'] = int(setting.lighting.speed)  # time_per_color is always int
            elif device.lc_driver_type == Hydro690Lc:
                kwargs['time_per_color'] = int(setting.lighting.speed)
                kwargs['speed'] = setting.lighting.speed  # speed is converted to int when needed by liquidctl
            else:
                kwargs['speed'] = setting.lighting.speed  # str for most modern devices
        if setting.lighting.backward:
            kwargs['direction'] = 'backward'
        _LOG.debug_lc("%s %s.set_color(%s)", _LC_CALLED, lc_device.__class__.__name__, kwargs)
        lc_device.set_color(**kwargs)

    @staticmethod
    def _set_screen(setting: Setting, lc_device: BaseDriver) -> None:
        """For set_screen we set several settings at once"""
        kwargs: Dict[str, Any] = {
            'channel': setting.channel_name,
        }

        def set_screen_kwargs(**kws) -> None:
            _LOG.debug_lc("%s %s.set_screen(%s)", _LC_CALLED, lc_device.__class__.__name__, kws)
            lc_device.set_screen(**kws)

        if setting.lcd.brightness is not None:
            kwargs['mode'] = "brightness"
            kwargs['value'] = setting.lcd.brightness
            set_screen_kwargs(**kwargs)
        if setting.lcd.orientation is not None:
            kwargs['mode'] = "orientation"
            kwargs['value'] = setting.lcd.orientation
            set_screen_kwargs(**kwargs)
        if setting.lcd.mode == "image" and setting.lcd.tmp_image_file is not None:
            image = Image.open(setting.lcd.tmp_image_file)
            if image.format is not None and image.format == "GIF":
                kwargs['mode'] = "gif"
            else:
                kwargs['mode'] = "static"
            image.close()
            kwargs['value'] = setting.lcd.tmp_image_file
            set_screen_kwargs(**kwargs)
        elif setting.lcd.mode == "liquid":
            kwargs['mode'] = setting.lcd.mode
            kwargs['value'] = None
            set_screen_kwargs(**kwargs)

    def reinitialize_devices(self) -> None:
        """This is helpful/necessary after waking from sleep for example"""
        if FeatureToggle.no_init:
            return
        _LOG.info("Reinitializing liquidctl devices")
        for _, lc_device in self._devices_drivers.values():
            _LOG.debug_lc("%s %s.initialize()", _LC_CALLED, lc_device.__class__.__name__)
            lc_device.initialize()
        _LOG.debug("All devices have been reinitialized")

    def _initialize_devices(self) -> None:
        _LOG.debug("Initializing Liquidctl devices")
        try:
            _LOG.debug_lc("%s liquidctl.find_liquidctl_devices()", _LC_CALLED)
            devices: List[BaseDriver] = list(liquidctl.find_liquidctl_devices())
            _LOG.debug_lc("liquidctl find_liquidctl_devices response: %s", devices)
            if FeatureToggle.testing:
                from coolero.repositories.test_repo_ext import TestRepoExtension
                TestRepoExtension.insert_test_mocks(devices)
                _LOG.debug_lc("TEST liquidctl find_liquidctl_devices response: %s", devices)
        except ValueError:  # ValueError can happen when no devices were found
            _LOG.warning('No Liquidctl devices detected')
            devices = []
        self._check_for_legacy_690(devices)
        try:
            for index, lc_device in enumerate(devices):
                if self._device_is_supported(lc_device):
                    if FeatureToggle.testing:
                        from coolero.repositories.test_repo_ext import TestRepoExtension
                        _LOG.debug_lc("%s %s.connect()", _LC_CALLED, lc_device.__class__.__name__)
                        TestRepoExtension.connect_mock(lc_device)
                    else:
                        _LOG.debug_lc("%s %s.connect()", _LC_CALLED, lc_device.__class__.__name__)
                        lc_device.connect()
                    if FeatureToggle.testing:
                        from coolero.repositories.test_repo_ext import TestRepoExtension
                        _LOG.debug_lc("%s %s.initialize()", _LC_CALLED, lc_device.__class__.__name__)
                        lc_init_status: List[Tuple] = TestRepoExtension.initialize_mock(lc_device)
                    elif FeatureToggle.no_init or isinstance(lc_device, AuraLed):
                        # initialization on the AuraLed device has the side effect of resetting all
                        # the LED settings and is currently not needed for this device in liquidctl.
                        lc_init_status = []
                    else:
                        _LOG.debug_lc("%s %s.initialize()", _LC_CALLED, lc_device.__class__.__name__)
                        lc_init_status: List[Tuple] = lc_device.initialize()
                    _LOG.debug_lc('liquidctl initialization response: %s: %s',
                                  lc_device.__class__.__name__, lc_init_status)
                    lc_device_id = index + 1
                    init_status = self._map_status(lc_device, lc_init_status, lc_device_id) \
                        if isinstance(lc_init_status, list) else Status()
                    device_info = self._extract_device_info(lc_device)
                    device = Device(
                        _name=lc_device.description,
                        _type_id=(DeviceType.LIQUIDCTL, lc_device_id),
                        _status_current=init_status,
                        _lc_driver_type=type(lc_device),
                        _lc_init_firmware_version=init_status.firmware_version,
                        _info=device_info
                    )
                    # get the status after initialization to fill with complete data right away
                    if FeatureToggle.testing:
                        from coolero.repositories.test_repo_ext import TestRepoExtension
                        TestRepoExtension.prepare_for_mocks_get_status(device, lc_device)
                    _LOG.debug_lc("%s %s.get_status()", _LC_CALLED, lc_device.__class__.__name__)
                    lc_status: List[Tuple] = lc_device.get_status()
                    _LOG.debug_lc("%s %s", _LC_RESPONSE, lc_status)
                    device.status = self._map_status(lc_device, lc_status, lc_device_id)
                    self._devices_drivers[lc_device_id] = (device, lc_device)
        except OSError as os_exc:  # OSError when device was found but there's a connection error (udev rules)
            _LOG.error('Device Communication Error', exc_info=os_exc)
            raise DeviceCommunicationError() from os_exc
        self._update_device_colors()  # This needs to be done after initialization & first real status
        self._check_for_legacy_kraken2_firmware()

    def _map_status(self, device: BaseDriver, lc_status: List[Tuple], device_id: int) -> Status:
        status_dict = self._convert_status_to_dict(lc_status)
        return self._device_info_extractor.extract_status_from(device, status_dict, device_id)

    @staticmethod
    def _convert_status_to_dict(lc_status: List[Tuple]) -> Dict[str, Union[str, int, float]]:
        return {
            str(lc_property).strip().lower(): value
            for lc_property, value, unit in lc_status
        }

    def _extract_device_info(self, device: BaseDriver) -> Optional[DeviceInfo]:
        return self._device_info_extractor.extract_info_from(device)

    def _device_is_supported(self, device: BaseDriver) -> bool:
        return self._device_info_extractor.is_device_supported(device)

    def _update_device_colors(self) -> None:
        number_of_colors: int = 0
        for device, _ in self._devices_drivers.values():
            number_of_colors += len(device.status.temps)
            number_of_colors += len(device.status.channels)
        colors = self._create_all_colors(number_of_colors)
        color_counter: int = 0
        for device, _ in self._devices_drivers.values():
            for temp_status in device.status.temps:
                device.colors[temp_status.name] = colors[color_counter]
                color_counter += 1
            for channel_status in device.status.channels:
                device.colors[channel_status.name] = colors[color_counter]
                color_counter += 1

    @staticmethod
    def _create_all_colors(number_of_colors: int) -> List[str]:
        if not number_of_colors:
            return []
        colors_selectors = numpy.linspace(0, 1, number_of_colors)
        color_map = matplotlib.cm.get_cmap('cool')(colors_selectors)
        return [matplotlib.cm.colors.to_hex(color) for color in color_map]

    @staticmethod
    def _check_for_legacy_690(devices: List[BaseDriver]) -> None:
        """Modern and Legacy Asetek 690Lc devices have the same device ID.
        We ask the user to verify which device is connected so that we can correctly communicate with the device"""
        legacy_690_count: int = 0
        for index, device_driver in enumerate(devices):
            if isinstance(device_driver, Modern690Lc):
                device_id = index + 1
                is_legacy_690_per_device: Dict[int, bool] = AppSettings.user.value(
                    UserSettings.LEGACY_690LC, defaultValue={})
                is_legacy_690: Optional[bool] = is_legacy_690_per_device.get(device_id)
                if is_legacy_690 is None:
                    is_legacy_690 = Legacy690Dialog(device_id).ask()
                if is_legacy_690:
                    if FeatureToggle.testing:
                        # This method doesn't seem to work as well as expected. At least the description is wrong.
                        # See https://gitlab.com/coolero/coolero/-/issues/19
                        devices[index] = device_driver.downgrade_to_legacy()
                    else:
                        devices[index] = Legacy690Lc.find_supported_devices()[legacy_690_count]
                    legacy_690_count += 1

    def _check_for_legacy_kraken2_firmware(self) -> None:
        """Older Kraken2 devices with old firmware don't support speed profiles"""
        for device, lc_device in self._devices_drivers.values():
            if isinstance(lc_device, Kraken2) and lc_device.device_type == lc_device.DEVICE_KRAKENX and (
                    (device.status.firmware_version and device.status.firmware_version.startswith('2.'))
                    or (device.lc_init_firmware_version and device.lc_init_firmware_version.startswith('2.'))
            ):
                LegacyKraken2FirmwareDialog().warn()

    def _handle_device_specific_shutdowns(self) -> None:
        for _, lc_device in self._devices_drivers.values():
            if isinstance(lc_device, CorsairHidPsu):
                _LOG.debug_lc("%s %s.initialize()", _LC_CALLED, lc_device.__class__.__name__)
                lc_device.initialize()  # attempt to reset fan control back to hardware
