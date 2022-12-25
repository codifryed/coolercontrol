#!/usr/bin/env python3

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

import argparse
import logging.config
import os
import pwd
import re
import shutil
import signal
import socket
import sys
import threading
from pathlib import Path
from re import Pattern
from socketserver import StreamRequestHandler, UnixStreamServer
from typing import List, Dict

from coolercontrol.models.base_daemon import BaseDaemon
from coolercontrol.settings import Settings

logging.config.fileConfig(Settings.app_path.joinpath('config/logging.conf'), disable_existing_loggers=False)
_LOG = logging.getLogger(__name__)


class MessageHandler(StreamRequestHandler, BaseDaemon):
    _supported_client_versions: List[str] = ['1']
    _pattern_hwmon_path: Pattern = re.compile(r'^.{1,100}?/hwmon/hwmon\d{1,3}?.{1,100}$')  # some basic path validation
    server_running: bool = True

    def handle(self) -> None:
        while self.server_running:
            try:
                msg: Dict = self.recv_dict()
                _LOG.debug('Message received: %s', msg)
                if version := msg.get('version'):
                    if version in self._supported_client_versions:
                        self.send_kwargs(response='version supported')
                        _LOG.info('Client version supported and greeting exchanged')
                    else:
                        self.send_kwargs(response='version NOT supported')
                        _LOG.info('Client version not supported: %s', msg)
                elif cmd := msg.get('cmd'):
                    if cmd == 'close connection':
                        self.send_kwargs(response='bye')
                        _LOG.info('Client closing connection')
                        break
                    elif cmd == 'shutdown':
                        self.send_kwargs(response='bye')
                        _LOG.info('Client initiated daemon shutdown')
                        threading.Thread(target=self.server.shutdown).start()
                        break
                    else:
                        self.send_kwargs(response='unknown command')
                        _LOG.warning('Unknown command received')
                elif path := msg.get('path'):
                    if value := msg.get('value'):
                        self._apply_hwmon_setting(path, value)
                    else:
                        _LOG.error('Invalid Message sent')
                else:
                    _LOG.error('Invalid Message sent')
            except (OSError, ValueError) as exc:
                _LOG.error('Unexpected socket error, closing connection', exc_info=exc)
                # Unexpected End of Message errors repeat in the loop until the service is killed,
                #  so we close the connection (break) to avoid that. (haven't seen a EoM recovery yet)
                break

    def _apply_hwmon_setting(self, path: str, value: str) -> None:
        try:
            if self._pattern_hwmon_path.match(path):
                return_code: int = Path(path).write_text(value)
                self.send_kwargs(response="setting success")
                _LOG.info("Successfully applied hwmon setting. Return code: %s", return_code)
            else:
                self.send_kwargs(response="invalid path")
                _LOG.error("Invalid path")
        except OSError as exc:
            _LOG.error("Error when trying to set hwmon values: \nPath: %s\nValue: %s", path, value, exc_info=exc)
            self.send_kwargs(response="setting failure")


class SystemDaemon:
    """
    This is a Daemon Service to be started with system startup and handle needed device communication and data
    collection.
    """
    _socket_name: str = 'coolercontrold.sock'
    _default_timeout: float = 1.0
    _systemd_first_socket_fd: int = 3

    def __init__(self) -> None:
        _LOG.info('System Daemon Starting...')
        parser = argparse.ArgumentParser(
            description='System Daemon to help monitor and control your cooling and other devices',
            exit_on_error=False
        )
        parser.add_argument(
            'username', metavar='USER', type=str, help='the user to allow access to the system daemon socket',
            nargs='?', default=''
        )
        parser.add_argument('--debug', action='store_true', help='turn on debug logging')
        args = parser.parse_args()
        if args.debug:
            logging.getLogger('root').setLevel(logging.DEBUG)
            _LOG.debug('DEBUG level enabled')

        if os.geteuid() != 0:
            _LOG.error('The system daemon must be run with superuser privileges')
            sys.exit(1)
        self._is_standard_daemon: bool = False
        username: str = args.username
        self._socket_dir: str = str(Settings.system_run_path.joinpath(self._socket_name))
        socket_exists: bool = Path(self._socket_dir).exists()
        if username and not socket_exists:
            self._init_standard_daemon(username)
        else:
            if not socket_exists:
                _LOG.error('SystemD socket not found')
                sys.exit(1)
            self._init_systemd_daemon()

    def _init_standard_daemon(self, username: str) -> None:
        _LOG.info('Initializing standard daemon')
        self._is_standard_daemon = True
        gid: int = pwd.getpwnam(username).pw_gid
        Path(Settings.system_run_path).mkdir(exist_ok=True, mode=0o775)
        self._server = UnixStreamServer(self._socket_dir, MessageHandler, bind_and_activate=True)
        self._server.timeout = self._default_timeout
        shutil.chown(self._socket_dir, group=gid)
        os.chmod(self._socket_dir, mode=0o775)
        shutil.chown(Settings.system_run_path, group=gid)
        os.chmod(Settings.system_run_path, mode=0o775)  # needs to be set again after socket creation

    def _init_systemd_daemon(self) -> None:
        _LOG.info('Initializing systemd daemon')
        os.chdir(Settings.system_run_path)  # set working folder - throws error if not setup by systemd
        self._server = UnixStreamServer(self._socket_dir, MessageHandler, bind_and_activate=False)
        self._server.timeout = self._default_timeout
        # alternative when more than 1 socket: use sys.stdin.fileno() to get FD from systemd directly
        self._server.socket = socket.fromfd(self._systemd_first_socket_fd, socket.AF_UNIX, socket.SOCK_STREAM)

    def run(self) -> None:
        signal.signal(signal.SIGINT, self.trigger_shutdown)
        signal.signal(signal.SIGTERM, self.trigger_shutdown)
        _LOG.info('System Daemon Listening...')
        self._server.serve_forever()

        self._server.server_close()
        if self._is_standard_daemon:
            Path(self._socket_dir).unlink(missing_ok=True)
        _LOG.info('System Daemon Shutdown')

    def trigger_shutdown(self, *args) -> None:
        _LOG.info('Attempting to shutdown gracefully')
        MessageHandler.server_running = False
        threading.Thread(target=self._server.shutdown).start()


def start_system_daemon() -> None:
    SystemDaemon().run()


if __name__ == "__main__":
    start_system_daemon()
