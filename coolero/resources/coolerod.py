#!/usr/bin/env python3

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

import logging
import os
import re
import shutil
import signal
import sys
from logging.handlers import RotatingFileHandler
from multiprocessing.connection import Listener
from pathlib import Path
from re import Pattern
from typing import List

_LOG = logging.getLogger(__name__)
_LOG_FILE: str = 'coolerod.log'
_SOCKET_NAME: str = 'coolerod.sock'


class SessionDaemon:
    """
    This class & script file is used as a simple daemon for regularly setting system hwmon values as a privileged user.
    Requires that at least Python 3.5 is installed on the system.
    Currently, used to create a user session daemon for portable installations like flatpak and appImage.
    """
    _pattern_hwmon_path: Pattern = re.compile(r'^.{1,100}?/hwmon/hwmon\d{1,3}?.{1,100}$')  # some basic path validation
    _pattern_client_version: Pattern = re.compile(r'^v\d{1,2}')

    def __init__(self, daemon_path: Path) -> None:
        log_filename: Path = daemon_path.joinpath(_LOG_FILE)
        file_handler = RotatingFileHandler(
            filename=log_filename, maxBytes=10485760, backupCount=1, encoding='utf-8'
        )
        log_formatter = logging.Formatter(fmt='%(asctime)s %(levelname)s: %(message)s')
        file_handler.setFormatter(log_formatter)
        logging.getLogger('root').setLevel(logging.INFO)
        logging.getLogger('root').addHandler(file_handler)
        self._ui_user: str = sys.argv[1]
        if not self._ui_user:
            raise ValueError('No Username given. The session daemon must connect to the current user.')
        self._key: bytes = self._ui_user.encode('utf-8')
        self._socket: str = str(daemon_path.joinpath(_SOCKET_NAME))
        self._conn = None
        self._listener = None
        self._running: bool = False
        _LOG.info('Coolero Daemon initialized')

    def run(self) -> None:
        signal.signal(signal.SIGINT, self.shutdown_gracefully)
        signal.signal(signal.SIGTERM, self.shutdown_gracefully)
        try:
            # if socket is already open from another instance, will exit
            self._listener = Listener(address=self._socket, family='AF_UNIX', authkey=self._key)
            shutil.chown(self._socket, user=self._ui_user)
            self._running = True
            _LOG.info('Coolero Daemon running')
            while self._running:
                self._conn = self._listener.accept()
                _LOG.info('Client Connection accepted')
                while self._running:
                    msg = self._conn.recv()
                    _LOG.debug('Message received: %s', msg)
                    if isinstance(msg, str):
                        if self._pattern_client_version.match(msg):
                            if msg == 'v1':
                                self._conn.send('version supported')
                                _LOG.info('Client version supported and greeting exchanged')
                            else:
                                self._conn.send('version NOT supported')
                                _LOG.info('Client version not supported: %s', msg)
                        elif msg == 'close connection':
                            self._conn.send('bye')
                            _LOG.info('Client closing connection')
                            self._conn.close()
                            self._conn = None
                            break
                        elif msg == 'shutdown':
                            self._conn.send('bye')
                            _LOG.info('Client initiated daemon shutdown')
                            self.shutdown_gracefully()
                            break
                    elif isinstance(msg, List):
                        self._apply_hwmon_setting(msg)
                    else:
                        _LOG.error('Invalid Message sent')
        except BaseException as exc:
            _LOG.error('Error creating or running Socket listener', exc_info=exc)
        self.shutdown()

    def _apply_hwmon_setting(self, msg: List) -> None:
        if len(msg) == 2:
            try:
                path = str(msg[0])
                if self._pattern_hwmon_path.match(path):
                    value = str(msg[1])
                    Path(path).write_text(value)
                    self._conn.send('setting success')
                    _LOG.info('Successfully applied hwmon setting')
                    return
                else:
                    _LOG.error('Invalid path')
            except BaseException as exc:
                _LOG.error('Error when trying to set hwmon values: %s', msg, exc_info=exc)
            self._conn.send('setting failure')
        else:
            _LOG.error('Invalid Message')

    def shutdown_gracefully(self, *args) -> None:
        _LOG.info('Attempting to shutdown gracefully')
        if self._conn is not None:
            self._conn.close()
            self._conn = None
        self._running = False
        self.shutdown()

    def shutdown(self):
        if self._listener is not None:
            self._listener.close()
        Path(self._socket).unlink(missing_ok=True)
        _LOG.info('Coolero Daemon Shutdown')


if __name__ == "__main__":
    # fork to create a completely separate running daemon process
    try:
        pid = os.fork()
        if pid == 0:  # if this is the child process, run it
            # To become the session leader of this new session and the process group
            # leader of the new process group, we call os.setsid().  The process is
            # also guaranteed not to have a controlling terminal.
            os.setsid()
            # This second fork guarantees that the child is no longer a session leader, preventing the daemon from ever
            # acquiring a controlling terminal.
            pid = os.fork()
            if pid == 0:
                daemon_dir: Path = Path(__file__).resolve().parent
                os.chdir(daemon_dir)  # set working folder
                os.umask(0o077)
                # cleanup parent connections for daemon
                os.open(os.devnull, os.O_RDWR)  # standard input (0)
                # Duplicate standard input to standard output and standard error.
                os.dup2(0, 1)  # standard output (1)
                os.dup2(0, 2)
                SessionDaemon(daemon_dir).run()
            else:
                os._exit(0)
        else:
            os._exit(0)
    except (OSError, ValueError) as err:
        print('Could not fork child process: ', err)
        sys.exit(1)
    sys.exit(0)
