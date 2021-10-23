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

from typing import Tuple


class ButtonUtils:

    @staticmethod
    def extract_info_from_channel_btn_id(channel_btn_id: str) -> Tuple[int, str]:
        # channel_btn_id looks like: btn_liquidctl_lc-device-id_channel-name
        parts = channel_btn_id.split('_')
        lc_device_id = int(parts[2])
        channel_name = str(parts[3])
        return lc_device_id, channel_name
