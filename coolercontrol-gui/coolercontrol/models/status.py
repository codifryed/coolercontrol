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

from dataclasses import dataclass, field
from datetime import datetime

import dateutil.parser
from dataclass_wizard import LoadMixin, JSONWizard
from dataclass_wizard.decorators import _single_arg_alias
from dataclass_wizard.type_def import N


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


class CustomDateTimeLoader(LoadMixin):
    @staticmethod
    @_single_arg_alias(dateutil.parser.isoparse)
    def load_to_datetime(o: str | N, _: type[datetime]) -> datetime:
        # alias: isoparse(o)
        ...


@dataclass(order=True, frozen=True)
class Status(JSONWizard, CustomDateTimeLoader):
    """A Model which contains various applicable device statuses"""

    timestamp: datetime = field(default_factory=datetime.now().astimezone, compare=True)
    firmware_version: str | None = field(default=None, compare=False)
    temps: list[TempStatus] = field(default_factory=list, compare=False)
    channels: list[ChannelStatus] = field(default_factory=list, compare=False)
