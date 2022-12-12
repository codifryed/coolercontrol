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

from dataclasses import dataclass

from pydantic import BaseModel

Statuses = list[tuple[str, str, str]]


class LiquidctlException(Exception):
    pass


# Dataclasses are used for fast ORJSON Response serialization, all others use Pydantic

@dataclass
class LiquidctlError:
    message: str


@dataclass
class DeviceProperties:
    speed_channels: list[str]
    color_channels: list[str]


@dataclass
class Device:
    id: int
    description: str
    device_type: str
    serial_number: str | None
    properties: DeviceProperties


class Handshake(BaseModel):
    shake: bool = False


class InitRequest(BaseModel):
    pump_mode: str | None


class FixedSpeedRequest(BaseModel):
    channel: str
    duty: int


class SpeedProfileRequest(BaseModel):
    channel: str
    profile: list[tuple[int, int]]
    temperature_sensor: int | None


class ColorRequest(BaseModel):
    channel: str
    mode: str
    colors: list[list[int]]
    time_per_color: int | None
    speed: str | None
    direction: str | None


class ScreenRequest(BaseModel):
    channel: str
    mode: str
    value: str | None
