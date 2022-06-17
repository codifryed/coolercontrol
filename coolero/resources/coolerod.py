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

# IMPORTANT: This whole file must be self-contained with no external dependencies.
#  The reason being that it's purpose is to enable hwmon write access for portable installations,
#  which can not rely on system-installed libraries (only std libraries)

import json
import logging
import os
import re
import shutil
import signal
import struct
import sys
import threading
from logging.handlers import RotatingFileHandler
from pathlib import Path
from re import Pattern
from socketserver import UnixStreamServer, StreamRequestHandler
from typing import List, Dict

_LOG = logging.getLogger(__name__)
_LOG_FILE: str = 'coolerod.log'


class MessageHandler(StreamRequestHandler):
    _header_format: str = '>Q'
    _header_size: int = 8
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
                break  # close the connection if we get EoM errors

    def send_kwargs(self, **kwargs) -> None:
        msg: bytes = json.dumps(kwargs).encode('utf-8')
        header: bytes = struct.pack(self._header_format, len(msg))
        self.request.sendall(header)
        self.request.sendall(msg)

    def recv_dict(self) -> Dict:
        """ May raise ValueError """
        header: bytes = self._recv_exact(self._header_size)
        msg_len: int = struct.unpack(self._header_format, header)[0]
        msg = self._recv_exact(msg_len)
        return json.loads(msg.decode('utf-8'))

    def _recv_exact(self, num_bytes: int) -> bytes:
        buf: bytearray = bytearray(num_bytes)
        view: memoryview = memoryview(buf)
        while num_bytes > 0:
            num_read_bytes: int = self.request.recv_into(view, num_bytes)
            view = view[num_read_bytes:]
            num_bytes -= num_read_bytes
            if num_read_bytes == 0:  # end of message, otherwise read some more
                if num_bytes == 0:  # everything went according to plan
                    break
                elif num_bytes < len(buf):
                    _LOG.error("daemon socket connection closed mid send")
                raise ValueError('Unexpected End Of Message')
        return bytes(buf)  # bytes type is immutable

    def _apply_hwmon_setting(self, path: str, value: str) -> None:
        try:
            if self._pattern_hwmon_path.match(path):
                Path(path).write_text(value)
                self.send_kwargs(response='setting success')
                _LOG.info('Successfully applied hwmon setting')
                return
            else:
                self.send_kwargs(response='invalid path')
                _LOG.error('Invalid path')
        except OSError as exc:
            _LOG.error('Error when trying to set hwmon values', exc_info=exc)
            self.send_kwargs(response='setting failure')


class SessionDaemon:
    """
    This class & script file is used as a simple daemon for regularly setting system hwmon values as a privileged user.
    Requires that at least Python 3.5 is installed on the system.
    Currently, used to create a user session daemon for portable installations like flatpak and appImage.
    """
    _socket_name: str = 'coolerod.sock'
    _default_timeout: float = 1.0

    def __init__(self, daemon_path: Path) -> None:
        log_filename: Path = daemon_path.joinpath(_LOG_FILE)
        file_handler = RotatingFileHandler(
            filename=log_filename, maxBytes=10485760, backupCount=1, encoding='utf-8'
        )
        log_formatter = logging.Formatter(fmt='%(asctime)s %(levelname)s: %(message)s')
        file_handler.setFormatter(log_formatter)
        logging.getLogger('root').setLevel(logging.INFO)
        logging.getLogger('root').addHandler(file_handler)
        self._user: str = sys.argv[1] if len(sys.argv) > 1 else ''
        if not self._user:
            raise ValueError(
                'No Username given. The session daemon socket only allows connections from the current user.')
        self._socket_dir: str = str(daemon_path.joinpath(self._socket_name))
        Path(self._socket_dir).unlink(missing_ok=True)  # make sure
        self._server = UnixStreamServer(self._socket_dir, MessageHandler, bind_and_activate=True)
        self._server.timeout = self._default_timeout
        shutil.chown(self._socket_dir, user=self._user)
        _LOG.info('Session Daemon initialized')

    def run(self) -> None:
        signal.signal(signal.SIGINT, self.trigger_shutdown)
        signal.signal(signal.SIGTERM, self.trigger_shutdown)
        self._server.serve_forever()

        self._server.server_close()
        Path(self._socket_dir).unlink(missing_ok=True)
        _LOG.info('Session Daemon Shutdown')

    def trigger_shutdown(self, *args) -> None:
        _LOG.info('Attempting to shutdown gracefully')
        MessageHandler.server_running = False
        threading.Thread(target=self._server.shutdown).start()


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
                os.umask(0o022)
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
