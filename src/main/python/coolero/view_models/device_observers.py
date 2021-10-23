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

from models.device_status import DeviceStatus


class DeviceSubject:
    """An Observable/Subject parent class for observing devices"""

    @property
    def device_statuses(self) -> list[DeviceStatus]:
        raise NotImplementedError("This method should be implemented in the child class")

    def subscribe(self, observer: 'DeviceObserver') -> None:
        """Subscribe to get notified of device changes"""
        raise NotImplementedError("This method should be implemented in the child class")

    def unsubscribe(self, observer: 'DeviceObserver') -> None:
        """Unsubscribe"""
        raise NotImplementedError("This method should be implemented in the child class")

    def notify(self) -> None:
        """Notify Observers of devices changes"""
        raise NotImplementedError("This method should be implemented in the child class")


class DeviceObserver:
    """An observer parent class, to be notified of any changes to the devices' status"""

    def notify(self, observable: DeviceSubject) -> None:
        raise NotImplementedError("This method should be implemented in the child class")
