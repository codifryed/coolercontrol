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
from typing import Dict, Any

import msgpack
import zmq
from zmq import SocketOption

from device_service import DeviceService
from liqctld import SOCKET_NAME

log = logging.getLogger(__name__)
SYSTEMD_SOCKET_FD: int = 3
TMP_SOCKET_DIR: str = f"/tmp/{SOCKET_NAME}"


class Server:

    def __init__(self, is_systemd: bool) -> None:
        self.device_service = DeviceService()
        self.context = zmq.Context()
        self.socket = self.context.socket(zmq.REP)
        if is_systemd:
            self.socket.setsockopt(SocketOption.USE_FD, SYSTEMD_SOCKET_FD)
        self.socket.bind(f"ipc://{TMP_SOCKET_DIR}")

        log.info("Server listening")
        while True:
            try:
                #  Wait for next request from client
                packed_msg = self.socket.recv()
                message: Dict[str, str] = msgpack.unpackb(packed_msg)
                log.debug(f"Received request: {message}")
                self.process(message)
            except KeyboardInterrupt:
                log.info("Interrupt Signal received. Quitting.")
                break
            except BaseException as e:
                log.error("Something went wrong unpacking the received message", exc_info=e)

        # clean up connection and quit
        self.socket.close()
        self.context.term()

    def process(self, message: Dict[str, str]) -> None:
        response: Dict[str, Any] = {}
        if message.get("handshake") is not None:
            response["handshake"] = 1
            log.info("Handshake exchanged")
        elif message.get("command") == "find_liquidctl_devices":
            self.device_service.initialize_devices()
        else:
            log.warning("Unknown Request")
        packed_msg: bytes = msgpack.packb(response)
        self.socket.send(packed_msg)
