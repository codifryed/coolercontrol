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

from typing import Callable, Optional

from PySide6.QtCore import QObject, Slot


class SettingsObserver(QObject):
    func_on_change: Optional[Callable]
    clear_graph_history_function: Optional[Callable]

    @staticmethod
    def connect_on_change(func: Callable) -> None:
        SettingsObserver.func_on_change = func

    @staticmethod
    def connect_clear_graph_history(func: Callable) -> None:
        SettingsObserver.clear_graph_history_function = func

    @staticmethod
    def clear_graph_history() -> None:
        if SettingsObserver.clear_graph_history_function is not None:
            SettingsObserver.clear_graph_history_function()

    @Slot()
    def settings_changed(self, setting_changed: str) -> None:
        if SettingsObserver.func_on_change is not None:
            self.func_on_change(setting_changed)
