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
import tempfile
from multiprocessing.connection import Client
from pathlib import Path
from time import sleep

_LOG = logging.getLogger(__name__)
_SOCKET_NAME: str = 'coolerod.sock'
_DEFAULT_RESPONSE_WAIT_TIME: float = 1.0


class HwmonDaemonClient:
    """
    This class is used to speak the Coolero Daemon running in the background
    """

    def __init__(self, key: bytes) -> None:
        sleep(0.1)  # just in case the daemon isn't loaded yet
        self._tmp_path = Path(tempfile.gettempdir()).joinpath('coolero')
        self._tmp_path.mkdir(mode=0o700, exist_ok=True)
        self._key: bytes = key
        self._socket: str = str(self._tmp_path.joinpath(_SOCKET_NAME))
        self._conn = Client(address=self._socket, family='AF_UNIX', authkey=self._key)
        self.greet_daemon()

    def greet_daemon(self) -> None:
        self._conn.send('hello')
        if self._conn.poll(_DEFAULT_RESPONSE_WAIT_TIME):
            response = self._conn.recv()
            if response != 'hello back':
                _LOG.error('Incorrect greeting response from coolerod: %s', response)
                raise ValueError('Incorrect greeting response from coolerod')
            _LOG.info('coolerod greeting exchange successful')
        else:
            raise ValueError('No greeting response from coolerod')

    def apply_setting(self, path: Path, value: str) -> bool:
        self._conn.send([str(path), value])
        if self._conn.poll(_DEFAULT_RESPONSE_WAIT_TIME):
            response = self._conn.recv()
            if response == 'setting success':
                return True
        return False

    def shutdown(self) -> None:
        self._conn.send('shutdown')
        self._conn.close()
