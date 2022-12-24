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
import socket
import struct
from typing import Callable, Any, Dict

_LOG = logging.getLogger(__name__)


class BaseDaemon:
    _socket_name: str = 'coolerod.sock'
    _header_format: str = '>Q'  # big endian
    _header_size: int = 8
    _default_timeout: float = 1.0

    def __init__(self) -> None:
        """Initialize the socket before connecting"""
        self._socket: socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._socket.settimeout(self._default_timeout)
        # this is a small workaround to allow this base class to work with both a server and a client
        self.request: socket = self._socket

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

    @staticmethod
    def log_exceptions(function: Callable, default_return: Any = None) -> Any:
        try:
            return function()
        except (OSError, ValueError) as exc:
            _LOG.error('Unexpected socket error', exc_info=exc)
            return default_return

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
