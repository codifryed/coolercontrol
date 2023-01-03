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

from coolercontrol.models.channel_info import ChannelInfo


@dataclass(frozen=True)
class DeviceInfo:
    channels: dict[str, ChannelInfo] = field(default_factory=dict)
    lighting_speeds: list[str] = field(default_factory=list)
    temp_min: int = 20
    temp_max: int = 100
    temp_ext_available: bool = False
    profile_max_length: int = 17  # reasonable default, one control point every 5 degrees for 20-100 range
    profile_min_length: int = 2
    model: str | None = None
