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
from dataclasses import dataclass
from typing import Dict

import zmq
from orjson import orjson
from zmq import SocketOption

from device_service import DeviceService

log = logging.getLogger(__name__)
SOCKET_NAME: str = "coolercontrol.sock"
SYSTEMD_SOCKET_FD: int = 3
TMP_SOCKET_DIR: str = f"/tmp/{SOCKET_NAME}"


@dataclass(frozen=True)
class Request:
    command: str = ""
    parameters: str = ""


@dataclass(frozen=True)
class Response:
    success: str = ""
    error: str = ""


class Server:

    def __init__(self, is_systemd: bool) -> None:
        self.device_service = DeviceService()
        self.running: bool = True
        self.context = zmq.Context()
        self.socket = self.context.socket(zmq.REP)
        if is_systemd:
            self.socket.setsockopt(SocketOption.USE_FD, SYSTEMD_SOCKET_FD)
        self.socket.bind(f"ipc://{TMP_SOCKET_DIR}")

        log.info("Listening...")
        while self.running:
            try:
                #  Wait for next request from client
                msg_json = self.socket.recv()

                json_dict: Dict = orjson.loads(msg_json)
                message: Request = Request(**json_dict)
                # deep debugging:
                # log.debug(f"Received request: {message}")
                self.process(message)
            except KeyboardInterrupt:
                log.info("Interrupt Signal received. Quitting.")
                break
            except BaseException as e:
                log.error("Something went wrong unpacking the received message", exc_info=e)
                break  # occasionally something goes very wrong. Either we should restart the server or exit

        # clean up connection and quit
        if not is_systemd:
            self.socket.close()
        self.context.term()

    def process(self, message: Request) -> None:
        try:
            if message.command == "handshake":
                log.info("Exchanging handshake")
                response = Response("handshake")
            elif message.command == "quit":
                log.info("Quit command received. Shutting down.")
                response = Response("quit")
                self.running = False
            elif message.command == "find_devices":
                devices_list = self.device_service.initialize_devices()
                if devices_list and devices_list[0].get("error") is not None:
                    response = Response(error=devices_list[0]["error"])
                else:
                    response = Response(str(orjson.dumps(devices_list).decode("utf-8")))
            elif message.command == "get_status" and message.parameters:
                lc_status = self.device_service.get_status(message.parameters)
                if lc_status and lc_status[0][0] == "error":
                    response = Response(error=lc_status[0][1])
                else:
                    response = Response(str(orjson.dumps(lc_status).decode("utf-8")))
            else:
                response = Response(error="Unknown Request")
                log.warning("Unknown Request")

            msg_json: bytes = orjson.dumps(response)
            self.socket.send(msg_json)
            # only for deep debugging:
            # log.debug(f"Response sent: {msg_json}")
        except BaseException as err:
            log.error("error by message serialization & sending:", exc_info=err)
