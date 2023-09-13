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

import logging.config
import time

from PySide6.QtCore import QThread
from apscheduler.job import Job
from jeepney import DBusAddress, MatchRule, message_bus
from jeepney.io.blocking import DBusConnection, open_dbus_connection, Proxy

from coolercontrol.settings import Settings, UserSettings

log = logging.getLogger(__name__)


class SleepListener(QThread):
    """
    This is a service that listens for a DBus message when the computer is put into sleep/hibernation and when it comes
    back out again. There are resets of some hardware systems when resuming and CoolerControl needs to re-apply
    all settings after waking up.
    This service extends and runs in its own QThread.
    """
    preparing_for_sleep_mode: bool = False
    _dbus_address_sleep: DBusAddress = DBusAddress('/org/freedesktop/login1',
                                                   bus_name='org.freedesktop.login1',
                                                   interface='org.freedesktop.login1.Manager')

    def __init__(self, device_update_jobs: list[Job]) -> None:
        super().__init__()
        self._device_update_jobs = device_update_jobs
        try:
            self._connection_system: DBusConnection = open_dbus_connection(bus='SYSTEM')
            self._match_rule = MatchRule(
                type="signal",
                interface=self._dbus_address_sleep.interface,
                member="PrepareForSleep",
                path=self._dbus_address_sleep.object_path
            )
            bus_proxy = Proxy(message_bus, self._connection_system)
            log.info("System DBus connection established: %s", bus_proxy.AddMatch(self._match_rule) == ())
            self.start()
        except BaseException as ex:
            log.error('Could not open DBus connection for listening', exc_info=ex)

    def run(self) -> None:
        with self._connection_system.filter(self._match_rule) as queue:  # if this errors out, terminate process
            while True:
                try:
                    log.debug("Listening...")
                    signal_msg = self._connection_system.recv_until_filtered(queue)
                    log.debug("DBus message received: %s ; %s", signal_msg.header, signal_msg.body)
                    if signal_msg.body[0]:  # returns true if entering sleep, false when waking
                        log.info("System is going to sleep/hibernating, pausing jobs")
                        SleepListener.preparing_for_sleep_mode = True
                        for job in self._device_update_jobs:
                            job.pause()
                    else:
                        log.info("System is resuming from sleep/hibernate, resuming jobs")
                        delay: int = Settings.user.value(UserSettings.STARTUP_DELAY, defaultValue=2, type=int)
                        # use startup delay in case usb connections take longer than normal
                        time.sleep(max(delay, 2))  # give the system at least a moment to wake up
                        log.debug("Resuming paused jobs after reinitialization")
                        for job in self._device_update_jobs:
                            job.resume()
                        time.sleep(1)  # give jobs a moment to process before waking up fully
                        SleepListener.preparing_for_sleep_mode = False
                except BaseException as ex:
                    log.error("Unexpected Error", exc_info=ex)

    def shutdown(self) -> None:
        if self.isRunning():
            self._connection_system.close()
            self.terminate()
            self.wait(3000)
        log.debug("Sleep DBus Listener shutdown")
