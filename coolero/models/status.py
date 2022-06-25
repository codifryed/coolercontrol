#  Coolero - monitor and control your cooling and other devices
#  Copyright (c) 2021  Guy Boldon
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
from datetime import datetime
from typing import Optional, List


@dataclass(frozen=True)
class TempStatus:
    name: str
    temp: float
    frontend_name: str
    external_name: str


@dataclass(frozen=True)
class ChannelStatus:
    name: str
    rpm: int | None = None
    duty: float | None = None
    pwm_mode: int | None = None


@dataclass(order=True, frozen=True)
class Status:
    """A Model which contains various applicable device statuses"""

    timestamp: datetime = field(default_factory=datetime.now, compare=True)
    firmware_version: str | None = field(default=None, compare=False)
    temps: List[TempStatus] = field(default_factory=list, compare=False)
    channels: List[ChannelStatus] = field(default_factory=list, compare=False)
