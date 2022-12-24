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
from collections import defaultdict
from datetime import datetime
from typing import Dict, Any, Optional, Tuple

from apscheduler.executors.pool import ThreadPoolExecutor
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.date import DateTrigger
from jeepney import DBusAddress, new_method_call, Message
from jeepney.io.blocking import open_dbus_connection, DBusConnection

from coolercontrol.settings import Settings, IS_FLATPAK, UserSettings
from coolercontrol.view.core.functions import Functions

_LOG = logging.getLogger(__name__)


class Notifications:
    _scheduler: BackgroundScheduler = BackgroundScheduler(
        executors={'default': ThreadPoolExecutor(1)},
        job_defaults={'misfire_grace_time': 3, 'coalesce': False, 'replace_existing': False, 'max_instances': 10}
    )
    _dbus_address_notifications: DBusAddress = DBusAddress('/org/freedesktop/Notifications',
                                                           bus_name='org.freedesktop.Notifications',
                                                           interface='org.freedesktop.Notifications')
    _dbus_method: str = 'Notify'
    _dbus_message_body_signature: str = 'susssasa{sv}i'
    _app_name: str = 'org.coolercontrol.CoolerControl'
    _title: str = Settings.app['app_name']
    _timeout_ms: int = 3000  # -1 = default set in desktop env
    _timeout_s: int = 3

    def __init__(self) -> None:
        self._scheduler.start()
        self._previous_message_ids: Dict[str, Tuple[int, datetime]] = defaultdict(lambda: (0, datetime.now()))
        if IS_FLATPAK:
            self._icon: str = self._app_name
        else:
            self._icon = Functions.set_image('logo_200.png')
        try:
            self._connection_session: DBusConnection = open_dbus_connection(bus='SESSION')
            _LOG.info("Notification DBus Connection established")
        except BaseException as ex:
            _LOG.error('Could not open DBus connection for notifications', exc_info=ex)

    def shutdown(self) -> None:
        self._scheduler.shutdown()
        if self._connection_session is not None:
            self._connection_session.close()
        _LOG.debug("Notification DBus Service shutdown")

    def settings_applied(self, device_name: str = '') -> None:
        """This will take the response of the applied-settings-function and send a notification of completion"""
        desktop_notifications_enabled: bool = Settings.user.value(
            UserSettings.DESKTOP_NOTIFICATIONS, defaultValue=True, type=bool
        )
        if not desktop_notifications_enabled or self._connection_session is None or device_name is None:
            return
        msg: str = 'Settings applied'
        if device_name:
            msg += f' to\n{device_name}'
        self._scheduler.add_job(
            lambda: self._send_message(msg, device_name),
            DateTrigger(),  # defaults to now()
        )

    def _send_message(self, msg: str, device_name: str) -> None:
        """
        Every desktop env handles notification a bit differently
        and not every desktop env properly pushes a new notification after they have timed-out
        For example:
          gnome doesn't respect all the settings but works well enough for now,
          KDE respects the settings but has changed its implementation over time,
            requiring manual management for a smoother UX (previous_message_id)
        """
        try:
            seconds_since_last_notification = (datetime.now() - self._previous_message_ids[device_name][1]).seconds
            previous_message_id: int = self._previous_message_ids[device_name][0] \
                if seconds_since_last_notification < self._timeout_s else 0  # force new notification after timeout
            dbus_msg: Message = new_method_call(
                self._dbus_address_notifications,
                self._dbus_method,
                self._dbus_message_body_signature,
                (
                    self._app_name,
                    previous_message_id,
                    self._icon,
                    self._title,
                    msg,
                    [], {},  # Actions, hints
                    self._timeout_ms,  # expire_timeout (-1 = default)
                )
            )
            reply: Message = self._connection_session.send_and_get_reply(dbus_msg)
            if reply.body is not None:
                message_id = self._safe_cast_to_int(reply.body)
                if message_id is not None:
                    self._previous_message_ids[device_name] = (message_id, datetime.now())
                    _LOG.debug('DBus Notification received with ID: %s', reply.body[0])
            else:
                _LOG.warning('DBus Notification response body was empty')
        except BaseException as ex:
            _LOG.error('DBus messaging error', exc_info=ex)

    @staticmethod
    def _safe_cast_to_int(body: Any) -> Optional[int]:
        try:
            return int(body[0])
        except ValueError:
            _LOG.warning('DBus Notification response was not an ID: %s', body[0])
            return None
