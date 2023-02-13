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
from http import HTTPStatus

import uvicorn
from fastapi import FastAPI, Request
from fastapi.responses import ORJSONResponse

from coolercontrol_liqctld.device_service import DeviceService
from coolercontrol_liqctld.models import Handshake, LiquidctlException, LiquidctlError, Statuses, InitRequest, \
    FixedSpeedRequest, SpeedProfileRequest, ColorRequest, ScreenRequest

SYSTEMD_SOCKET_FD: int = 3
DEFAULT_PORT: int = 11986  # 11987 is the gui std port
log = logging.getLogger(__name__)
api = FastAPI()
device_service = DeviceService()


@api.exception_handler(LiquidctlException)
async def liquidctl_exception_handler(request: Request, exc: LiquidctlException) -> ORJSONResponse:
    return ORJSONResponse(
        status_code=HTTPStatus.INTERNAL_SERVER_ERROR,
        content=LiquidctlError(message=str(exc))
    )


@api.get("/handshake")
async def handshake():
    log.info("Exchanging handshake")
    return Handshake(shake=True)


@api.get("/devices", response_class=ORJSONResponse)
def get_devices() -> ORJSONResponse:
    devices = device_service.get_devices()
    return ORJSONResponse({"devices": devices})


@api.post("/devices/connect")
def connect_devices():
    """No longer necessary to call this endpoint. This is handled automatically in GET /devices"""
    device_service.connect_devices()
    return {"connected": True}


@api.put("/devices/{device_id}/legacy690", response_class=ORJSONResponse)
def set_device_as_legacy690(device_id: int) -> ORJSONResponse:
    device = device_service.set_device_as_legacy690(device_id)
    return ORJSONResponse(device)


@api.put("/devices/{device_id}/speed/fixed", response_class=ORJSONResponse)
def set_fixed_speed(device_id: int, speed_request: FixedSpeedRequest) -> ORJSONResponse:
    speed_kwargs = speed_request.dict(exclude_none=True)
    device_service.set_fixed_speed(device_id, speed_kwargs)
    return ORJSONResponse({"set_fixed_speed": True})


@api.put("/devices/{device_id}/speed/profile", response_class=ORJSONResponse)
def set_fixed_speed(device_id: int, speed_request: SpeedProfileRequest) -> ORJSONResponse:
    speed_kwargs = speed_request.dict(exclude_none=True)
    device_service.set_speed_profile(device_id, speed_kwargs)
    return ORJSONResponse({"set_speed_profile": True})


@api.put("/devices/{device_id}/color", response_class=ORJSONResponse)
def set_color(device_id: int, color_request: ColorRequest) -> ORJSONResponse:
    color_kwargs = color_request.dict(exclude_none=True)
    device_service.set_color(device_id, color_kwargs)
    return ORJSONResponse({"set_color": True})


@api.put("/devices/{device_id}/screen", response_class=ORJSONResponse)
def set_color(device_id: int, screen_request: ScreenRequest) -> ORJSONResponse:
    screen_kwargs = screen_request.dict(exclude_none=False)  # need None value for liquid mode
    device_service.set_screen(device_id, screen_kwargs)
    return ORJSONResponse({"set_screen": True})


@api.post("/devices/{device_id}/initialize", response_class=ORJSONResponse)
def init_device(device_id: int, init_request: InitRequest) -> ORJSONResponse:
    init_args = init_request.dict(exclude_none=True)
    status: Statuses = device_service.initialize_device(device_id, init_args)
    return ORJSONResponse({"status": status})


@api.get("/devices/{device_id}/status", response_class=ORJSONResponse)
def get_status(device_id: int) -> ORJSONResponse:
    status: Statuses = device_service.get_status(device_id)
    return ORJSONResponse({"status": status})


@api.post("/devices/disconnect")
def disconnect_all():
    """Not necessary to call this explicitly, /quit should be called in most situations and handles disconnects"""
    device_service.disconnect_all()
    return {"disconnected": True}


@api.post("/quit")
async def quit_server():
    log.info("Quit command received. Shutting down.")
    os.kill(os.getpid(), signal.SIGTERM)
    return {"quit": True}


class Server:

    def __init__(self, version: str, is_systemd: bool, log_level: int) -> None:
        self.is_systemd: bool = is_systemd
        self.log_level = logging.getLevelName(log_level).lower()
        self.log_config = uvicorn.config.LOGGING_CONFIG
        if is_systemd:
            self.log_config["formatters"]["default"]["fmt"] = \
                "%(levelname)-8s uvicorn - %(message)s"
            self.log_config["formatters"]["access"]["fmt"] = \
                '%(levelname)-8s uvicorn - %(client_addr)s - "%(request_line)s" %(status_code)s'
        else:
            self.log_config["formatters"]["default"]["fmt"] = \
                "%(asctime)-15s %(levelname)-8s uvicorn - %(message)s"
            self.log_config["formatters"]["access"]["fmt"] = \
                '%(asctime)-15s %(levelname)-8s uvicorn - %(client_addr)s - "%(request_line)s" %(status_code)s'
        api.version = version
        api.debug = log_level <= 10

    def startup(self) -> None:
        log.info("Liqctld server starting...")
        uvicorn.run(
            "coolercontrol_liqctld.server:api", host="127.0.0.1", port=DEFAULT_PORT, workers=1,
            use_colors=True, log_level=self.log_level, log_config=self.log_config
        )

    @staticmethod
    @api.on_event("shutdown")
    def shutdown() -> None:
        log.info("Liqctld server shutting down")
        device_service.shutdown()
