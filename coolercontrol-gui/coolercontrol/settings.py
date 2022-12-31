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

import json
import logging
import os
import tempfile
from enum import Enum
from pathlib import Path

from PySide6 import QtCore
from PySide6.QtCore import QSettings

from coolercontrol.models.lcd_mode import LcdMode
from coolercontrol.models.lighting_mode import LightingMode
# noinspection PyUnresolvedReferences
from coolercontrol.models.saved_lcd_settings import SavedLcd, ChannelLcdSettings, LcdModeSettings, LcdModeSetting
# noinspection PyUnresolvedReferences
from coolercontrol.models.saved_lighting_settings import SavedLighting, ChannelLightingSettings, ModeSettings, \
    ModeSetting
# noinspection PyUnresolvedReferences
from coolercontrol.models.saved_speed_settings import SavedProfiles, ChannelSettings, TempSourceSettings, DeviceSetting, \
    ProfileSetting
from coolercontrol.models.speed_profile import SpeedProfile
from coolercontrol.xdg import XDG

log = logging.getLogger(__name__)
IS_APP_IMAGE: bool = os.environ.get('APPDIR') is not None
IS_WAYLAND: bool = os.environ.get('WAYLAND_DISPLAY') is not None and os.environ.get('QT_QPA_PLATFORM') != 'xcb'
IS_GNOME: bool = 'GNOME' in XDG.xdg_current_desktop()
_COOLER_CONTROL_SUB_DIR: str = '/coolercontrol/'


def serialize(path: Path, settings: dict) -> None:
    with open(path, "w", encoding='utf-8') as write:
        json.dump(settings, write, indent=2)


def deserialize(path: Path) -> dict:
    with open(path, "r", encoding='utf-8') as reader:
        return dict(json.loads(reader.read()))


class UserSettings(str, Enum):
    SAVE_WINDOW_SIZE = "save_window_size"
    WINDOW_SIZE = "window_size"
    WINDOW_POSITION = "window_position"
    ENABLE_LIGHT_THEME = "enable_light_theme"
    ENABLE_BRIGHT_TRAY_ICON = "enable_bright_tray_icon"
    HIDE_ON_CLOSE = "hide_on_close"
    HIDE_ON_MINIMIZE = "hide_on_minimize"
    START_MINIMIZED = "start_minimized"
    STARTUP_DELAY = "startup_delay"
    UI_SCALE_FACTOR = "ui_scale_factor"
    ENABLE_SMOOTHING = "enable_smoothing"
    ENABLE_DYNAMIC_TEMP_HANDLING = "enable_dynamic_temp_handling"
    ENABLE_COMPOSITE_TEMPS = "enable_composite_temps"
    CHECK_FOR_UPDATES = "check_for_updates"
    PROFILES = "profiles/v1"
    APPLIED_PROFILES = "applied_profiles/v1"
    LIGHTING_SETTINGS = "lighting_settings/v1"
    LCD_SETTINGS = "lcd_settings/v1"
    OVERVIEW_LEGEND_HIDDEN_LINES = "overview_legend_hidden_lines"
    LOAD_APPLIED_AT_BOOT = "load_applied_at_boot"
    DESKTOP_NOTIFICATIONS = "desktop_notifications"
    ENABLE_HWMON_FILTER = "enable_hwmon_filter"
    ENABLE_HWMON_TEMPS = "enable_hwmon_temps"
    MENU_OPEN = "menu_open"

    def __str__(self) -> str:
        return str.__str__(self)


class Settings:
    """This class provides static Settings access to all files in the application"""
    app_path: Path = Path(__file__).resolve().parent
    tmp_path: Path = Path(f'{tempfile.gettempdir()}{_COOLER_CONTROL_SUB_DIR}')
    if os.geteuid() != 0:  # system daemon shouldn't create this directory
        tmp_path.mkdir(mode=0o700, exist_ok=True)
    system_run_path: Path = Path(f'/run{_COOLER_CONTROL_SUB_DIR}')
    user_run_path: Path = Path(f'{XDG.xdg_runtime_dir()}{_COOLER_CONTROL_SUB_DIR}')
    user_config_path: Path = Path(f'{XDG.xdg_config_home()}{_COOLER_CONTROL_SUB_DIR}')
    user: QSettings = QtCore.QSettings('coolercontrol', 'coolercontrol-v1')
    app: dict = {}
    theme: dict = {}
    _saved_profiles: SavedProfiles = user.value(UserSettings.PROFILES, defaultValue=SavedProfiles())
    _last_applied_profiles: SavedProfiles = user.value(  # type: ignore
        UserSettings.APPLIED_PROFILES, defaultValue=SavedProfiles())
    _saved_lighting_settings: SavedLighting = user.value(  # type: ignore
        UserSettings.LIGHTING_SETTINGS, defaultValue=SavedLighting())
    _saved_lcd_settings = user.value(
        UserSettings.LCD_SETTINGS, defaultValue=SavedLcd())
    _overview_legend_hidden_lines: set[str] = user.value(UserSettings.OVERVIEW_LEGEND_HIDDEN_LINES, defaultValue=set())

    _app_json_path = app_path.joinpath('resources/settings.json')
    if not _app_json_path.is_file():
        log.fatal('FATAL: "settings.json" not found! check in the folder %s', _app_json_path)
    app = deserialize(_app_json_path)

    user_theme: str = "default"
    is_light_theme = user.value(UserSettings.ENABLE_LIGHT_THEME, defaultValue=False, type=bool)
    if is_light_theme:
        user_theme = "bright_theme"
    _theme_json_path = app_path.joinpath(f'resources/themes/{user_theme}.json')
    if not _theme_json_path.is_file():
        log.warning('"gui/themes/%s.json" not found! check in the folder %s', user_theme, _theme_json_path)
    theme = deserialize(_theme_json_path)

    @staticmethod
    def save_profiles() -> None:
        log.debug('Saving Profiles')
        Settings.user.setValue(UserSettings.PROFILES, Settings._saved_profiles)
        # sync is needed for when multiple settings are saved from multiple threads, not to run into thread lock/freeze
        Settings.user.sync()

    @staticmethod
    def save_lighting_settings() -> None:
        log.debug('Saving Lighting Settings')
        Settings.user.setValue(UserSettings.LIGHTING_SETTINGS, Settings._saved_lighting_settings)
        Settings.user.sync()

    @staticmethod
    def save_lcd_settings() -> None:
        log.debug('Saving LCD Settings')
        Settings.user.setValue(UserSettings.LCD_SETTINGS, Settings._saved_lcd_settings)
        Settings.user.sync()

    @staticmethod
    def save_last_applied_profiles() -> None:
        log.debug('Saving Last Applied Profiles')
        Settings.user.setValue(UserSettings.APPLIED_PROFILES, Settings._last_applied_profiles)
        Settings.user.sync()

    @staticmethod
    def get_temp_source_settings(
            device_name: str, device_id: int, channel_name: str
    ) -> TempSourceSettings:
        saved_profiles = Settings._saved_profiles.profiles
        device_setting = DeviceSetting(device_name, device_id)
        return saved_profiles[device_setting].channels[channel_name]

    @staticmethod
    def get_temp_source_chosen_profile(
            device_name: str, device_id: int, channel_name: str, temp_source_name: str
    ) -> ProfileSetting | None:
        return Settings.get_temp_source_settings(
            device_name, device_id, channel_name
        ).chosen_profile.get(temp_source_name)

    @staticmethod
    def get_temp_source_profiles(
            device_name: str, device_id: int, channel_name: str, temp_source_name: str
    ) -> list[ProfileSetting]:
        return Settings.get_temp_source_settings(
            device_name, device_id, channel_name
        ).profiles[temp_source_name]

    @staticmethod
    def save_chosen_profile_for_temp_source(
            device_name: str, device_id: int, channel_name: str, temp_source_name: str, speed_profile: SpeedProfile
    ) -> None:
        temp_source_settings = Settings.get_temp_source_settings(device_name, device_id, channel_name)
        temp_source_settings.chosen_profile[temp_source_name] = ProfileSetting(speed_profile)
        Settings.save_profiles()

    @staticmethod
    def save_fixed_profile(
            device_name: str, device_id: int, channel_name: str, temp_source_name: str, fixed_duty: int,
            pwm_mode: int | None
    ) -> None:
        temp_source_settings = Settings.get_temp_source_settings(device_name, device_id, channel_name)
        temp_source_settings.chosen_profile[temp_source_name] = ProfileSetting(
            SpeedProfile.FIXED, fixed_duty=fixed_duty, pwm_mode=pwm_mode)
        for profile in temp_source_settings.profiles[temp_source_name]:
            if profile.speed_profile == SpeedProfile.FIXED:
                profile.fixed_duty = fixed_duty
                profile.pwm_mode = pwm_mode
                break
        else:
            temp_source_settings.profiles[temp_source_name].append(
                ProfileSetting(SpeedProfile.FIXED, fixed_duty=fixed_duty, pwm_mode=pwm_mode)
            )
        Settings.save_profiles()

    @staticmethod
    def save_custom_profile(
            device_name: str, device_id: int, channel_name: str, temp_source_name: str,
            temps: list[int], duties: list[int], pwm_mode: int | None
    ) -> None:
        temp_source_settings = Settings.get_temp_source_settings(device_name, device_id, channel_name)
        temp_source_settings.chosen_profile[temp_source_name] = ProfileSetting(
            SpeedProfile.CUSTOM, profile_temps=temps, profile_duties=duties, pwm_mode=pwm_mode
        )
        for profile in temp_source_settings.profiles[temp_source_name]:
            if profile.speed_profile == SpeedProfile.CUSTOM:
                profile.profile_temps = temps
                profile.profile_duties = duties
                profile.pwm_mode = pwm_mode
                break
        else:
            temp_source_settings.profiles[temp_source_name].append(
                ProfileSetting(SpeedProfile.CUSTOM, profile_temps=temps, profile_duties=duties, pwm_mode=pwm_mode)
            )
        Settings.save_profiles()

    @staticmethod
    def get_last_applied_temp_source_settings(
            device_name: str, device_id: int, channel_name: str
    ) -> TempSourceSettings:
        last_applied_profiles = Settings._last_applied_profiles.profiles
        device_setting = DeviceSetting(device_name, device_id)
        return last_applied_profiles[device_setting].channels[channel_name]

    @staticmethod
    def get_last_applied_profile_for_channel(
            device_name: str, device_id: int, channel_name: str
    ) -> tuple[str, ProfileSetting] | None:
        return Settings.get_last_applied_temp_source_settings(device_name, device_id, channel_name).last_profile

    @staticmethod
    def save_applied_fixed_profile(
            device_name: str, device_id: int, channel_name: str, temp_source_name: str, applied_fixed_duty: int,
            applied_pwm_mode: int | None
    ) -> None:
        last_applied_temp_source_settings = Settings.get_last_applied_temp_source_settings(
            device_name, device_id, channel_name)
        last_applied_temp_source_settings.last_profile = (
            temp_source_name, ProfileSetting(
                SpeedProfile.FIXED, fixed_duty=applied_fixed_duty, pwm_mode=applied_pwm_mode
            )
        )
        Settings.save_last_applied_profiles()

    @staticmethod
    def save_applied_custom_profile(
            device_name: str, device_id: int, channel_name: str, temp_source_name: str,
            temps: list[int], duties: list[int], pwm_mode: int | None
    ) -> None:
        last_applied_temp_source_settings = Settings.get_last_applied_temp_source_settings(
            device_name, device_id, channel_name)
        last_applied_temp_source_settings.last_profile = (
            temp_source_name, ProfileSetting(
                SpeedProfile.CUSTOM, profile_temps=temps, profile_duties=duties, pwm_mode=pwm_mode
            )
        )
        Settings.save_last_applied_profiles()

    @staticmethod
    def save_applied_none_default_profile(
            device_name: str, device_id: int, channel_name: str, temp_source_name: str, speed_profile: SpeedProfile,
            pwm_mode: int | None
    ) -> None:
        last_applied_temp_source_settings = Settings.get_last_applied_temp_source_settings(
            device_name, device_id, channel_name)
        last_applied_temp_source_settings.last_profile = (
            temp_source_name, ProfileSetting(speed_profile, pwm_mode=pwm_mode)
        )

    @staticmethod
    def clear_applied_profile_for_channel(
            device_name: str, device_id: int, channel_name: str
    ) -> None:
        Settings.get_last_applied_temp_source_settings(
            device_name, device_id, channel_name
        ).last_profile = None
        Settings.save_last_applied_profiles()

    @staticmethod
    def get_lighting_mode_settings_for_channel(device_name: str, device_id: int, channel_name: str) -> ModeSettings:
        return Settings._saved_lighting_settings.device_settings[
            DeviceSetting(device_name, device_id)].channels[channel_name]

    @staticmethod
    def get_lcd_mode_settings_for_channel(device_name: str, device_id: int, channel_name: str) -> LcdModeSettings:
        return Settings._saved_lcd_settings.device_settings[
            DeviceSetting(device_name, device_id)].channels[channel_name]

    @staticmethod
    def get_lighting_mode_setting_for_mode(
            device_name: str, device_id: int, channel_name: str, mode: LightingMode
    ) -> ModeSetting:
        return Settings.get_lighting_mode_settings_for_channel(device_name, device_id, channel_name).all[mode]

    @staticmethod
    def get_lcd_mode_setting_for_mode(
            device_name: str, device_id: int, channel_name: str, mode: LcdMode
    ) -> LcdModeSetting:
        return Settings.get_lcd_mode_settings_for_channel(device_name, device_id, channel_name).all[mode]

    @staticmethod
    def is_overview_line_visible(line_name: str) -> bool:
        return line_name not in Settings._overview_legend_hidden_lines

    @staticmethod
    def overview_line_is_visible(line_name: str, is_visible: bool) -> None:
        if is_visible:
            Settings._overview_legend_hidden_lines.discard(line_name)
        else:
            Settings._overview_legend_hidden_lines.add(line_name)
        Settings.user.setValue(UserSettings.OVERVIEW_LEGEND_HIDDEN_LINES, Settings._overview_legend_hidden_lines)

    @staticmethod
    def save_app_settings() -> None:
        """
        This is just a helper function for doing things like updating the version per script.
        This should not be called during the normal run of the application
        """
        if not Settings.app:
            return
        serialize(Settings._app_json_path, Settings.app)


class FeatureToggle:
    no_init: bool = False
    testing: bool = False
