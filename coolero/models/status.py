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
from typing import Optional


@dataclass(order=True, frozen=True)
class Status:
    """A Model which contains various applicable device statuses"""

    timestamp: datetime = field(default_factory=datetime.now, compare=True)
    device_temperature: Optional[float] = field(default=None, compare=False)
    load_percent: Optional[float] = field(default=None, compare=False)
    firmware_version: Optional[str] = field(default=None, compare=False)
    liquid_temperature: Optional[float] = field(default=None, compare=False)
    fan_rpm: Optional[int] = field(default=None, compare=False)
    fan_duty: Optional[float] = field(default=None, compare=False)
    pump_rpm: Optional[int] = field(default=None, compare=False)
    pump_duty: Optional[float] = field(default=None, compare=False)
    device_description: Optional[str] = field(default=None, compare=False)
