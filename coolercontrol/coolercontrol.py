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

# nuitka-project: --standalone
# nuitka-project: --follow-imports
# nuitka-project: --include-data-dir=config=coolercontrol_data/config
# nuitka-project: --include-data-dir=resources=coolercontrol_data/resources
# nuitka-project: --plugin-enable=pyside6,pylint-warnings,numpy
# nuitka-project: --include-module=services.liquidctl_device_extractors
# nuitka-project: --static-libpython=yes
# nuitka-project: --lto=no
# nuitka-project: --prefer-source-code
# nuitka-project: --python-flag=-S,-O,no_docstrings
# nuitka-project: --linux-onefile-icon=metadata/org.coolercontrol.coolercontrol.png

import argparse
import importlib.metadata
import logging.config
import os
import platform
import sys
import textwrap
import time
import traceback
from logging.handlers import RotatingFileHandler
from typing import Optional, Tuple

import setproctitle
from PySide6 import QtCore
from PySide6.QtCore import QTimer, QCoreApplication, QEvent, QSize, QPoint
from PySide6.QtGui import QColor, Qt, QIcon, QAction, QShortcut, QKeySequence, QHideEvent, QShowEvent
from PySide6.QtWidgets import QMainWindow, QGraphicsDropShadowEffect, QApplication, QSystemTrayIcon, QMenu, QMessageBox

from coolercontrol.app_instance import ApplicationInstance
from coolercontrol.dialogs.quit_dialog import QuitDialog
from coolercontrol.dialogs.udev_rules_dialog import UDevRulesDialog
from coolercontrol.exceptions.device_communication_error import DeviceCommunicationError
from coolercontrol.services.app_updater import AppUpdater
from coolercontrol.services.dynamic_buttons import DynamicButtons
from coolercontrol.services.shell_commander import ShellCommander
from coolercontrol.settings import Settings, UserSettings, IS_APP_IMAGE, FeatureToggle
from coolercontrol.view.core.functions import Functions
from coolercontrol.view.uis.pages.info_page import InfoPage
from coolercontrol.view.uis.pages.settings_page import SettingsPage
from coolercontrol.view.uis.windows.main_window import SetupMainWindow, UI_MainWindow, MainFunctions
from coolercontrol.view.uis.windows.splash_screen.splash_screen_style import SPLASH_SCREEN_STYLE
from coolercontrol.view.uis.windows.splash_screen.ui_splash_screen import Ui_SplashScreen  # type: ignore
from coolercontrol.view_models.devices_view_model import DevicesViewModel


def add_log_level() -> None:
    debug_lc_lvl: int = 15

    def log_for_level(self, message, *args, **kwargs) -> None:
        if self.isEnabledFor(debug_lc_lvl):
            self._log(debug_lc_lvl, message, args, **kwargs)

    def log_to_root(message, *args, **kwargs) -> None:
        logging.log(debug_lc_lvl, message, *args, **kwargs)

    logging.addLevelName(debug_lc_lvl, 'DEBUG_LC')
    setattr(logging, 'DEBUG_LC', debug_lc_lvl)
    setattr(logging, 'debug_lc', log_to_root)
    setattr(logging.getLoggerClass(), 'debug_lc', log_for_level)


logging.config.fileConfig(Settings.app_path.joinpath('config/logging.conf'), disable_existing_loggers=False)
add_log_level()
_LOG = logging.getLogger(__name__)
_APP: QApplication
_INIT_WINDOW: QMainWindow
_ICON: QIcon
_RUNNING_INSTANCE: ApplicationInstance


class Initialize(QMainWindow):

    def __init__(self) -> None:
        super().__init__()
        _LOG.info("CoolerControl is initializing...")
        self._load_progress_counter: int = 0

        self.app_settings = Settings.app
        self.user_settings = Settings.user
        self.user_settings.setValue('version', self.app_settings['version'])
        QApplication.setApplicationName(self.app_settings['app_name'])
        QApplication.setApplicationVersion(self.app_settings['version'])
        self.theme = Settings.theme

        parser = argparse.ArgumentParser(
            description='monitor and control your cooling and other devices',
            exit_on_error=False,
            formatter_class=argparse.RawTextHelpFormatter
        )
        parser.add_argument(
            '-v', '--version', action='version',
            version=f'\n {self._system_info()}'
        )
        parser.add_argument('--debug', action='store_true',
                            help='enable debug output\n'
                                 'a log file is created under /tmp/coolercontrol/\n'
                                 'for Flatpak installations see documentation')
        parser.add_argument('--debug-liquidctl', action='store_true', help='enable liquidctl debug output\n'
                                                                           'a log file is created same as above')
        parser.add_argument('--add-udev-rules', action='store_true', help='add recommended udev rules to the system')
        parser.add_argument('--export-profiles', action='store_true',
                            help='export the last applied profiles for each device and channel')
        parser.add_argument("--no-init", action="store_true",
                            help="skip device initialization if possible. \n"
                                 "WARNING this should only be used if you are already initializing your devices at "
                                 "startup")
        args = parser.parse_args()
        if args.add_udev_rules:
            successful: bool = ShellCommander.apply_udev_rules()
            if successful:
                parser.exit()
            else:
                parser.error('failed to add udev rules')
        if args.export_profiles:
            self._export_profiles(parser)
        # allow the above cli options before forcing a single running instance
        _verify_single_running_instance()
        if args.debug:
            log_filename = Settings.tmp_path.joinpath('coolercontrol.log')
            file_handler = RotatingFileHandler(
                filename=log_filename, maxBytes=10485760, backupCount=5, encoding='utf-8'
            )
            log_formatter = logging.getLogger('root').handlers[0].formatter
            file_handler.setFormatter(log_formatter)
            logging.getLogger('root').setLevel(logging.DEBUG)
            logging.getLogger('root').addHandler(file_handler)
            logging.getLogger('matplotlib').setLevel(logging.INFO)
            logging.getLogger('matplotlib').addHandler(file_handler)
            logging.getLogger('apscheduler').setLevel(logging.INFO)
            logging.getLogger('apscheduler').addHandler(file_handler)
            logging.getLogger('liquidctl').setLevel(logging.DEBUG)
            logging.getLogger('liquidctl').addHandler(file_handler)
            _LOG.debug('DEBUG level enabled\n%s', self._system_info())
        elif args.debug_liquidctl:
            log_filename = Settings.tmp_path.joinpath('coolercontrol.log')
            file_handler = RotatingFileHandler(
                filename=log_filename, maxBytes=10485760, backupCount=5, encoding='utf-8'
            )
            log_formatter = logging.getLogger('root').handlers[0].formatter
            file_handler.setFormatter(log_formatter)
            logging.getLogger('root').setLevel(logging.DEBUG_LC)
            logging.getLogger('root').addHandler(file_handler)
            logging.getLogger('liquidctl').setLevel(logging.DEBUG)
            logging.getLogger('liquidctl').addHandler(file_handler)
            _LOG.debug_lc('Liquidctl DEBUG_LC level enabled\n%s', self._system_info())
        if args.no_init:
            FeatureToggle.no_init = True

        # Setup splash window
        self.ui = Ui_SplashScreen()
        self.ui.setupUi(self)
        splash_style = SPLASH_SCREEN_STYLE.format(
            _bg_color=self.theme["app_color"]["bg_one"],
            _title_color=self.theme["app_color"]["text_title"],
            _color=self.theme["app_color"]["text_foreground"],
            _progress_bg_color=self.theme["app_color"]["bg_two"],
            _progress_color=self.theme["app_color"]["white"],
            _progress_from_color=self.theme["app_color"]["icon_hover"],
            _progress_to_color=self.theme["app_color"]["context_pressed"]
        )
        self.ui.dropShadowFrame.setStyleSheet(splash_style)
        self.ui.label_title.setStyleSheet(splash_style)
        self.ui.label_description.setStyleSheet(splash_style)
        self.ui.label_loading.setStyleSheet(splash_style)
        self.ui.label_version.setStyleSheet(splash_style)
        self.ui.progressBar.setStyleSheet(splash_style)
        self.setWindowFlag(Qt.FramelessWindowHint)
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.shadow = QGraphicsDropShadowEffect(self)
        self.shadow.setBlurRadius(20)
        self.shadow.setXOffset(0)
        self.shadow.setYOffset(0)
        self.shadow.setColor(QColor(0, 0, 0, 60))
        self.ui.dropShadowFrame.setGraphicsEffect(self.shadow)

        self.ui.label_loading.setText("<strong>Initializing</strong>")
        self.ui.label_version.setText(f'<strong>version</strong>: {self.app_settings["version"]}')

        if Settings.user.value(UserSettings.START_MINIMIZED, defaultValue=False, type=bool):
            if not Settings.user.value(UserSettings.HIDE_ON_MINIMIZE, defaultValue=False, type=bool):
                self.showMinimized()
        else:
            self.show()

        self.timer = QTimer()
        self.timer.timeout.connect(self.init_devices)
        self.timer.start(10)

    def init_main_window(self):
        self.main = MainWindow()
        self.main.devices_view_model = DevicesViewModel()
        self.main.dynamic_buttons = DynamicButtons(
            self.main.devices_view_model,
            self.main
        )

    def _system_info(self) -> str:
        sys_info: str = textwrap.dedent(f'''
        {self.app_settings["app_name"]} {self.app_settings["version"]}
        
        System:''')
        if platform.system() == 'Linux':
            sys_info += f'\n    {platform.freedesktop_os_release().get("PRETTY_NAME")}'  # type: ignore
        pyside_version: str = self._get_package_version("pyside6_essentials")
        if pyside_version == "unknown":
            pyside_version = self._get_package_version("pyside6")
        sys_info += textwrap.dedent(f'''
            {platform.platform()}
        
        Dependency versions:
            Python     {platform.python_version()}
            Liquidctl  {self._get_package_version("liquidctl")}
            Hidapi     {self._get_package_version("hidapi")}
            Pyusb      {self._get_package_version("pyusb")}
            Pillow     {self._get_package_version("pillow")}
            Smbus      {self._get_package_version("smbus")}
            PySide6    {pyside_version}
            Matplotlib {self._get_package_version("matplotlib")}
            Numpy      {self._get_package_version("numpy")}
        ''')
        return sys_info

    @staticmethod
    def _get_package_version(package_name: str) -> str:
        """This searches for package versions.
        First it checks the metadata, which is present for all packages.
        If the metadata isn't found, like with the compiled AppImage, it checks inside the package for __version__.
        If package doesn't exist then it either defaults to the last known version or "unknown"
        """
        try:
            return importlib.metadata.version(package_name)
        except importlib.metadata.PackageNotFoundError:
            match package_name:
                case "liquidctl":
                    import liquidctl
                    return Initialize._get_version_attribute(liquidctl)
                case "hidapi":
                    return ">=0.12.0.post2"
                case "pyusb":
                    return ">=1.2.1"
                case "pillow":
                    import PIL
                    return Initialize._get_version_attribute(PIL)
                case "smbus":
                    return ">=1.1.post2"
                case "pyside6":
                    import PySide6
                    return Initialize._get_version_attribute(PySide6)
                case "matplotlib":
                    import matplotlib
                    return Initialize._get_version_attribute(matplotlib)
                case "numpy":
                    import numpy
                    return Initialize._get_version_attribute(numpy)
                case _:
                    return "unknown"

    @staticmethod
    def _get_version_attribute(package_object: object) -> str:
        return getattr(package_object, "__version__", "unknown")

    @staticmethod
    def _export_profiles(parser: argparse.ArgumentParser) -> None:
        from collections import defaultdict
        import re
        import json
        print('\nExporting last applied profiles:\n-------------------------------------------------------------------')
        exported_profiles = defaultdict(dict)
        for device, channel_settings in Settings._last_applied_profiles.profiles.items():
            for channel_name, temp_source_setting in channel_settings.channels.items():
                if temp_source_setting.last_profile is not None:
                    temp_source_name, profile_setting = temp_source_setting.last_profile
                    profile_setting_repr = {}
                    if profile_setting.fixed_duty is not None:
                        profile_setting_repr['duty (%)'] = profile_setting.fixed_duty
                    elif profile_setting.profile_temps:
                        liquidctl_profile_style = str(
                            list(zip(profile_setting.profile_temps, profile_setting.profile_duties))
                        )
                        liquidctl_profile_style = re.sub(r'[\[\](,]', '', liquidctl_profile_style) \
                            .replace(')', ' ').strip()
                        profile_setting_repr['profile (temp duty)'] = liquidctl_profile_style
                    exported_profiles[device.name][channel_name] = {
                        temp_source_name: {
                            profile_setting.speed_profile: profile_setting_repr
                        }
                    }
        print(json.dumps(exported_profiles, indent=2))
        parser.exit()

    def init_devices(self) -> None:
        try:
            _APP.setQuitOnLastWindowClosed(False)  # splash and dialog windows at startup
            should_check_for_update: bool = self.user_settings.value(
                UserSettings.CHECK_FOR_UPDATES, defaultValue=False, type=bool
            ) and IS_APP_IMAGE
            if self._load_progress_counter == 0:
                self.ui.label_loading.setText("<strong>Startup</strong> delay")
            elif self._load_progress_counter == 2:
                if delay := Settings.user.value(UserSettings.STARTUP_DELAY, defaultValue=0, type=int):
                    time.sleep(delay)
                # we initialize the main window after the startup delay to help with system tray issues
                self.init_main_window()

            elif self._load_progress_counter == 5:
                self.main.devices_view_model.schedule_status_updates()

                if should_check_for_update:
                    self.ui.label_loading.setText("<strong>Checking</strong> for updates")
            elif self._load_progress_counter == 10:
                if should_check_for_update:
                    AppUpdater.run(self)

                self.ui.label_loading.setText("<strong>Initializing</strong> CPU connection")
            elif self._load_progress_counter == 20:
                self.main.devices_view_model.init_cpu_repo()

                self.ui.label_loading.setText("<strong>Initializing</strong> GPU Connection")
            elif self._load_progress_counter == 35:
                self.main.devices_view_model.init_gpu_repo()

                self.ui.label_loading.setText("<strong>Initializing</strong> Liquidctl devices")
            elif self._load_progress_counter == 50:
                try:
                    self.main.devices_view_model.init_liquidctl_repo()
                except DeviceCommunicationError as ex:
                    _LOG.error("Liquidctl device communication error: %s", ex)
                    UDevRulesDialog(self).run()
                except BaseException as exc:
                    _LOG.error("Unexpected Liquidctl initialization error: %s", exc, exc_info=exc)

                self.ui.label_loading.setText("<strong>Initializing</strong> Hwmon devices")
            elif self._load_progress_counter == 65:
                try:
                    self.main.devices_view_model.init_hwmon_repo()
                except BaseException as ex:
                    _LOG.error("Unexpected Hwmon error: %s", ex, exc_info=ex)

                self.ui.label_loading.setText("<strong>Initializing</strong> the UI")
            elif self._load_progress_counter == 75:
                # finalize repo setup
                self.main.devices_view_model.init_scheduler_commander()
                if Settings.user.value(UserSettings.ENABLE_COMPOSITE_TEMPS, defaultValue=False, type=bool):
                    self.main.devices_view_model.init_composite_repo()
                # wire up core logic:
                self.main.devices_view_model.subscribe(self.main.ui.system_overview_canvas)
                self.main.dynamic_buttons.create_menu_buttons_from_devices()
                self.main.ui.left_column.menus.info_page_layout.addWidget(
                    InfoPage(self.main.devices_view_model.devices)
                )
                self.main.ui.left_column.menus.settings_page_layout.addWidget(SettingsPage())

            elif self._load_progress_counter >= 100:
                self.timer.stop()
                _LOG.info("Displaying Main UI Window...")
                if Settings.user.value(UserSettings.START_MINIMIZED, defaultValue=False, type=bool):
                    if Settings.user.value(UserSettings.HIDE_ON_MINIMIZE, defaultValue=False, type=bool):
                        self.main.ui.system_overview_canvas.pause()  # pause animations at startup if hidden
                    else:
                        self.main.showMinimized()
                else:
                    self.main.show()
                self.close()

            self._load_progress_counter += 1
            self.ui.progressBar.setValue(self._load_progress_counter)
        except BaseException as ex:
            _LOG.fatal('Unexpected Error', exc_info=ex)
            _LOG.info("Shutting down...")
            _APP.setQuitOnLastWindowClosed(True)
            self.main.devices_view_model.shutdown()
            self.close()


class MainWindow(QMainWindow):

    def __init__(self) -> None:
        super().__init__()
        sys.excepthook = self.log_uncaught_exception
        self.ui = UI_MainWindow()
        self.ui.setup_ui(self)
        self.active_left_sub_menu: str = ''
        self.devices_view_model: DevicesViewModel = None  # type: ignore
        self.dynamic_buttons: DynamicButtons = None  # type: ignore

        self.app_settings = Settings.app
        self.user_settings = Settings.user

        SetupMainWindow.setup_gui(self)

        # restore window size & position
        if self.user_settings.contains(UserSettings.WINDOW_SIZE):
            try:
                self.resize(  # type: ignore
                    self.user_settings.value(
                        UserSettings.WINDOW_SIZE,
                        defaultValue=QSize(self.app_settings["startup_size"][0], self.app_settings["startup_size"][1]),
                        type=QSize
                    )
                )
                self.move(  # type: ignore
                    self.user_settings.value(
                        UserSettings.WINDOW_POSITION,
                        defaultValue=QPoint(200, 200),
                        type=QPoint
                    )
                )
                _LOG.debug('Loaded saved window size')
            except BaseException as ex:
                _LOG.error('Unable to get and restore saved window geometry: %s', ex)

        tray_icon_style = 'white' \
            if Settings.user.value(UserSettings.ENABLE_BRIGHT_TRAY_ICON, defaultValue=False, type=bool) \
            else 'color'
        tray_icon = QIcon(Functions.set_svg_image(f'logo_{tray_icon_style}.svg'))
        tray_icon.setIsMask(True)
        self.tray_menu = QMenu(self)
        self.tray_menu.addAction(
            QAction(
                self.app_settings['app_name'], self, icon=QIcon(tray_icon), triggered=None, enabled=False
            )  # type: ignore[call-overload]
        )
        self.tray_menu.addSeparator()
        self.tray_menu.addAction(  # shortcut='Ctrl+h' - shortcuts don't appear to work for the sys tray actions?
            QAction('&Show', self, triggered=self.show_main_window))  # type: ignore[call-overload]
        self.tray_menu.addAction(
            QAction('&Quit', self, triggered=self.force_close))  # type: ignore[call-overload]
        self.tray = QSystemTrayIcon(self)
        self.tray.setIcon(tray_icon)
        self.tray.setVisible(True)
        self.tray.setContextMenu(self.tray_menu)

        self.shortcut_close = QShortcut(QKeySequence('Ctrl+Q'), self)
        self.shortcut_close.activated.connect(self.force_close)
        self.shortcut_hide = QShortcut(QKeySequence('Ctrl+H'), self)
        self.shortcut_hide.activated.connect(self.hide)
        self.shortcut_toggle_menu = QShortcut(QKeySequence('Ctrl+/'), self)
        self.shortcut_toggle_menu.activated.connect(self.ui.left_menu.toggle_animation)

    def show_main_window(self) -> None:
        if not self.isVisible():
            self.setVisible(True)
        self.activateWindow()

    # main menu btn
    def btn_clicked(self) -> None:

        btn = SetupMainWindow.setup_btns(self)
        btn_id = btn.objectName()
        _LOG.debug('Button %s, clicked!', btn_id)

        # home btn
        if btn_id == "btn_system":
            self.ui.left_menu.select_only_one(btn.objectName())
            self.clear_left_sub_menu()
            MainFunctions.set_page(self, self.ui.load_pages.system_overview)
            self.ui.system_overview_canvas.redraw_workaround()

        # Info and Settings combined:
        elif btn_id in ["btn_settings", "btn_info"]:
            if not MainFunctions.left_column_is_visible(self):
                self.dynamic_buttons.uncheck_all_channel_buttons()
                MainFunctions.toggle_left_column(self)
                self.ui.left_menu.select_only_one_tab(btn_id)
                self.active_left_sub_menu = btn_id
            elif btn_id == self.active_left_sub_menu:
                # close side menu
                self.ui.left_menu.deselect_all_tab()
                self.active_left_sub_menu = ''
                MainFunctions.toggle_left_column(self)
            else:
                self.active_left_sub_menu = btn.objectName()
                self.ui.left_menu.select_only_one_tab(btn.objectName())

            if btn_id == "btn_settings":
                MainFunctions.set_left_column_menu(
                    self,
                    menu=self.ui.left_column.menus.settings_page,
                    title="Settings",
                    icon_path=Functions.set_svg_icon("icon_settings.svg")
                )
            elif btn_id == "btn_info":
                MainFunctions.set_left_column_menu(
                    self,
                    menu=self.ui.left_column.menus.info_page,
                    title="Info",
                    icon_path=Functions.set_svg_icon("icon_info.svg")
                )
        else:
            self.dynamic_buttons.set_device_page(btn_id)

    def clear_left_sub_menu(self) -> None:
        self.ui.left_menu.deselect_all_tab()
        if MainFunctions.left_column_is_visible(self):
            MainFunctions.toggle_left_column(self)

    def btn_released(self) -> None:
        btn = SetupMainWindow.setup_btns(self)
        _LOG.debug('Button %s, released!', btn.objectName())

    def hideEvent(self, event: QHideEvent) -> None:
        """improved efficiency by pausing animations & line calculations when window is hidden"""
        self.ui.system_overview_canvas.pause()
        if MainFunctions.device_column_is_visible(self):
            MainFunctions.toggle_device_column(self)
        self.dynamic_buttons.uncheck_all_channel_buttons()

    def showEvent(self, event: QShowEvent) -> None:
        if self.ui.system_overview_canvas.event_source:
            self.ui.system_overview_canvas.event_source.interval = 100
        self.ui.system_overview_canvas.resume()

    def resizeEvent(self, event: QEvent) -> None:
        SetupMainWindow.resize_grips(self)
        if self.ui.device_column_frame.width() > 0:
            self.ui.device_column_frame.setMinimumWidth(int((self.width() - self.ui.left_menu_frame.width()) / 2))

    def closeEvent(self, event: QEvent) -> None:
        """Shutdown or minimize to tray"""
        _APP.setQuitOnLastWindowClosed(True)  # set the safe default again, just in case of an unexpected crash
        if self.user_settings.value(UserSettings.HIDE_ON_CLOSE, defaultValue=False, type=bool):
            self.hide()
            event.ignore()
        else:
            self.shutdown(event)

    def force_close(self) -> None:
        if self.user_settings.value(UserSettings.HIDE_ON_CLOSE, defaultValue=False, type=bool):
            self.shutdown()
        else:
            self.close()

    def shutdown(self, event: Optional[QEvent] = None) -> None:
        """Shutdown process"""
        if Settings.user.value(UserSettings.CONFIRM_EXIT, defaultValue=True, type=bool):
            _APP.setQuitOnLastWindowClosed(False)
            reply = QuitDialog(self).run()
            _APP.setQuitOnLastWindowClosed(True)
        else:
            reply = QMessageBox.Yes
        if reply == QMessageBox.Yes:
            _LOG.info("Shutting down...")
            self.devices_view_model.shutdown()
            if self.user_settings.value(UserSettings.SAVE_WINDOW_SIZE, defaultValue=True, type=bool):
                if not self.isMaximized():  # do not save maximized size
                    self.user_settings.setValue(UserSettings.WINDOW_SIZE, self.size())
                    self.user_settings.setValue(UserSettings.WINDOW_POSITION, self.pos())
                    _LOG.debug('Saved window size in user settings')
            else:
                self.user_settings.remove(UserSettings.WINDOW_SIZE)
                self.user_settings.remove(UserSettings.WINDOW_POSITION)
            self.close()
            _APP.quit()
        elif event is not None:
            event.ignore()

    @staticmethod
    def log_uncaught_exception(*exc_info: Tuple) -> None:
        text = "".join(traceback.format_exception(*exc_info))
        _LOG.error('Unexpected error has occurred: %s', text)


def _verify_single_running_instance() -> None:
    global _RUNNING_INSTANCE
    _RUNNING_INSTANCE = ApplicationInstance()


def main() -> None:
    setproctitle.setproctitle("coolercontrol")
    QCoreApplication.setAttribute(QtCore.Qt.AA_ShareOpenGLContexts)
    QApplication.setAttribute(QtCore.Qt.AA_EnableHighDpiScaling)
    QApplication.setAttribute(QtCore.Qt.AA_UseHighDpiPixmaps)
    QApplication.setAttribute(Qt.AA_UseDesktopOpenGL)
    QApplication.setAttribute(Qt.AA_Use96Dpi)
    os.environ['QT_FONT_DPI'] = '96'  # this appears to need to be set to keep things sane
    os.environ['QT_SCALE_FACTOR'] = str(  # scale performs better than higher dpi
        Settings.user.value(UserSettings.UI_SCALE_FACTOR, defaultValue=1.0, type=float)
    )
    if Settings.app["custom_title_bar"]:
        os.environ['QT_WAYLAND_DISABLE_WINDOWDECORATION'] = '1'  # to make sure wayland doesn't do system decorations
    global _APP, _ICON, _INIT_WINDOW
    _APP = QApplication(sys.argv)
    _ICON = QIcon(Functions.set_svg_image('logo_color.svg'))
    _APP.setWindowIcon(_ICON)
    _INIT_WINDOW = Initialize()
    sys.exit(_APP.exec())


if __name__ == "__main__":
    main()
