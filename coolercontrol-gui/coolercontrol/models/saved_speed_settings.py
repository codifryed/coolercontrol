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

from collections import defaultdict
from dataclasses import dataclass, field

from coolercontrol.models.speed_profile import SpeedProfile


@dataclass
class ProfileSetting:
    speed_profile: SpeedProfile
    fixed_duty: int | None = None
    profile_temps: list[int] = field(default_factory=list)
    profile_duties: list[int] = field(default_factory=list)
    pwm_mode: int | None = None


@dataclass
class TempSourceSettings:
    profiles: dict[str, list[ProfileSetting]] = field(default_factory=lambda: defaultdict(list))
    chosen_profile: dict[str, ProfileSetting] = field(default_factory=dict)
    last_profile: tuple[str, ProfileSetting] | None = None


@dataclass
class ChannelSettings:
    channels: dict[str, TempSourceSettings] = field(default_factory=lambda: defaultdict(TempSourceSettings))


@dataclass(frozen=True, order=True)
class DeviceSetting:
    name: str
    id: int


@dataclass
class SavedProfiles:
    profiles: dict[DeviceSetting, ChannelSettings] = field(default_factory=lambda: defaultdict(ChannelSettings))
