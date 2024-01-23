#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2023  Guy Boldon
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
import signal
import subprocess
import threading
from http import HTTPStatus
from typing import List

import uvicorn
from coolercontrol_liqctld.device_service import DeviceService
from coolercontrol_liqctld.models import (
    ColorRequest,
    Device,
    FixedSpeedRequest,
    Handshake,
    InitRequest,
    LiquidctlError,
    LiquidctlException,
    ScreenRequest,
    SpeedProfileRequest,
    Statuses,
)
from fastapi import FastAPI, Request, status
from fastapi.responses import JSONResponse

SYSTEMD_SOCKET_FD: int = 3
SOCKET_ADDRESS: str = "/run/coolercontrol-liqctld.sock"
log = logging.getLogger(__name__)
api = FastAPI()
device_service = DeviceService()


@api.exception_handler(LiquidctlException)
async def liquidctl_exception_handler(
    _request: Request, exc: LiquidctlException
) -> JSONResponse:
    return JSONResponse(
        status_code=HTTPStatus.INTERNAL_SERVER_ERROR,
        content=LiquidctlError(message=str(exc)).json(),
    )


@api.get("/handshake")
async def handshake():
    log.info("Exchanging handshake")
    return Handshake(shake=True)


@api.get("/devices")
def get_devices():
    devices: List[Device] = device_service.get_devices()
    return {"devices": devices}


@api.post("/devices/connect")
def connect_devices() -> JSONResponse:
    """No longer necessary to call this endpoint. This is handled automatically in GET /devices"""
    device_service.connect_devices()
    return JSONResponse(
        status_code=status.HTTP_200_OK,
        content={},
    )


@api.put("/devices/{device_id}/legacy690")
def set_device_as_legacy690(device_id: int) -> Device:
    device: Device = device_service.set_device_as_legacy690(device_id)
    return device


@api.put("/devices/{device_id}/speed/fixed")
def set_fixed_speed(device_id: int, speed_request: FixedSpeedRequest) -> JSONResponse:
    speed_kwargs = speed_request.dict(exclude_none=True)
    device_service.set_fixed_speed(device_id, speed_kwargs)
    # empty success response needed for systemd socket service to not error on 0 byte content
    return JSONResponse(status_code=status.HTTP_200_OK, content={})


@api.put("/devices/{device_id}/speed/profile")
def set_speed_profile(
    device_id: int, speed_request: SpeedProfileRequest
) -> JSONResponse:
    speed_kwargs = speed_request.dict(exclude_none=True)
    device_service.set_speed_profile(device_id, speed_kwargs)
    return JSONResponse(status_code=status.HTTP_200_OK, content={})


@api.put("/devices/{device_id}/color")
def set_color(device_id: int, color_request: ColorRequest) -> JSONResponse:
    color_kwargs = color_request.dict(exclude_none=True)
    device_service.set_color(device_id, color_kwargs)
    return JSONResponse(status_code=status.HTTP_200_OK, content={})


@api.put("/devices/{device_id}/screen")
def set_screen(device_id: int, screen_request: ScreenRequest) -> JSONResponse:
    screen_kwargs = screen_request.dict(
        exclude_none=False
    )  # need None value for liquid mode
    device_service.set_screen(device_id, screen_kwargs)
    return JSONResponse(status_code=status.HTTP_200_OK, content={})


@api.post("/devices/{device_id}/initialize")
def init_device(device_id: int, init_request: InitRequest):
    init_args = init_request.dict(exclude_none=True)
    status_response: Statuses = device_service.initialize_device(device_id, init_args)
    return {"status": status_response}


@api.get("/devices/{device_id}/status")
def get_status(device_id: int):
    status_response: Statuses = device_service.get_status(device_id)
    return {"status": status_response}


@api.post("/devices/disconnect")
def disconnect_all() -> JSONResponse:
    """Not necessary to call this explicitly, /quit should be called in most situations and handles disconnects"""
    device_service.disconnect_all()
    return JSONResponse(status_code=status.HTTP_200_OK, content={})


@api.post("/quit")
async def quit_server() -> JSONResponse:
    log.info("Quit command received. Shutting down.")
    os.kill(os.getpid(), signal.SIGTERM)
    return JSONResponse(status_code=status.HTTP_200_OK, content={})


class Server:
    def __init__(self, version: str, is_systemd: bool, log_level: int) -> None:
        self.is_systemd: bool = is_systemd
        self.log_level = logging.getLevelName(log_level).lower()
        self.log_config = uvicorn.config.LOGGING_CONFIG
        if is_systemd:
            self.log_config["formatters"]["default"][
                "fmt"
            ] = "%(levelname)-8s uvicorn - %(message)s"
            self.log_config["formatters"]["access"][
                "fmt"
            ] = '%(levelname)-8s uvicorn - %(client_addr)s - "%(request_line)s" %(status_code)s'
        else:
            self.log_config["formatters"]["default"][
                "fmt"
            ] = "%(asctime)-15s %(levelname)-8s uvicorn - %(message)s"
            self.log_config["formatters"]["access"][
                "fmt"
            ] = '%(asctime)-15s %(levelname)-8s uvicorn - %(client_addr)s - "%(request_line)s" %(status_code)s'
        api.version = version
        api.debug = log_level <= 10

    def startup(self) -> None:
        log.info("Liqctld server starting...")
        # Restricts socket permissions further after uvicorn creates it.
        # We use a thread here to avoid left-over processes.
        chmod = f"sleep 2 && chmod 660 {SOCKET_ADDRESS}"
        process_kwargs = {
            "stdout": subprocess.DEVNULL,
            "stderr": subprocess.DEVNULL,
            "check": True,
            "shell": True,
        }
        threading.Thread(
            target=subprocess.run, args=(chmod,), kwargs=process_kwargs
        ).start()
        # systemd socket activation is not working as we want and requires extra steps,
        # so we let uvicorn handle socket creation.
        # socket_config = {'fd': SYSTEMD_SOCKET_FD} if self.is_systemd else {'uds': SOCKET_ADDRESS}
        uvicorn.run(
            "coolercontrol_liqctld.server:api",
            uds=SOCKET_ADDRESS,
            host="127.0.0.1",  # default host, used in the header even for uds
            workers=1,
            use_colors=True,
            log_level=self.log_level,
            log_config=self.log_config,
        )

    @staticmethod
    @api.on_event("shutdown")
    def shutdown() -> None:
        log.info("Liqctld server shutting down")
        device_service.shutdown()
