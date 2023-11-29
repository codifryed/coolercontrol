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

from dataclasses import dataclass, field
from typing import Optional

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
    speed_channels: list[str] = field(default_factory=list)
    color_channels: list[str] = field(default_factory=list)
    supports_cooling: Optional[bool] = None
    supports_cooling_profiles: Optional[bool] = None
    supports_lighting: Optional[bool] = None
    led_count: Optional[int] = None


@dataclass
class Device:
    id: int
    description: str
    device_type: str
    serial_number: Optional[str]
    properties: DeviceProperties


class Handshake(BaseModel):
    shake: bool = False


class InitRequest(BaseModel):
    pump_mode: Optional[str]


class FixedSpeedRequest(BaseModel):
    channel: str
    duty: int


class SpeedProfileRequest(BaseModel):
    channel: str
    profile: list[tuple[int, int]]
    temperature_sensor: Optional[int]


class ColorRequest(BaseModel):
    channel: str
    mode: str
    colors: list[list[int]]
    time_per_color: Optional[int]
    speed: Optional[str]
    direction: Optional[str]


class ScreenRequest(BaseModel):
    channel: str
    mode: str
    value: Optional[str]
