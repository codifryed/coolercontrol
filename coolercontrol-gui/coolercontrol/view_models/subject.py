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

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from coolercontrol.view_models.observer import Observer


class Subject:

    def subscribe(self, observer: Observer) -> None:
        """Subscribe to get notified of changes"""
        raise NotImplementedError("This method should be implemented in the child class")

    def unsubscribe(self, observer: Observer) -> None:
        """Unsubscribe"""
        raise NotImplementedError("This method should be implemented in the child class")

    def notify_observers(self) -> None:
        """Notify Observers of changes"""
        raise NotImplementedError("This method should be implemented in the child class")
