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

from functools import lru_cache

from numpy import ndarray

from coolercontrol.models.device import DeviceType


class ButtonUtils:

    @staticmethod
    @lru_cache(maxsize=128)
    def extract_info_from_channel_btn_id(channel_btn_id: str) -> tuple[int, str, DeviceType]:
        """Utility method to extract the parts from the channel_btn_id String
        channel_btn_id looks like: btn_liquidctl_lc-device-id_channel-name"""
        parts = channel_btn_id.split('_')
        device_type_str = str(parts[1])
        if device_type_str == 'gpu':
            device_type = DeviceType.GPU
        elif device_type_str == 'hwmon':
            device_type = DeviceType.HWMON
        else:
            device_type = DeviceType.LIQUIDCTL
        if len(parts) < 3:
            return -1, '', device_type
        lc_device_id = int(parts[2])
        if len(parts) == 3:
            return lc_device_id, '', device_type
        channel_name = str(parts[3])
        return lc_device_id, channel_name, device_type


class MathUtils:

    @staticmethod
    def convert_axis_to_profile(temps: list[int], duties: list[int]) -> list[tuple[int, int]]:
        """Converts two axis to a list of pairs"""
        return list(zip(temps, duties))

    @staticmethod
    def norm_profile(
            profile: list[tuple[int, int]],
            critical_temp: int,
            max_duty_value: int = 100
    ) -> list[tuple[int, int]]:
        """
        Sort, cleanup and set safety levels for the given profile
        """
        profile = sorted(list(profile) + [(critical_temp, max_duty_value)], key=lambda p: (p[0], -p[1]))
        mono = profile[:1]
        for (x, y), (xb, yb) in zip(profile[1:], profile[:-1]):
            if x == xb:
                continue
            if y < yb:
                y = yb
            mono.append((x, y))
            if y == max_duty_value:
                break
        return mono

    @staticmethod
    def interpol_profile(profile: list[tuple[int, int]], temp: float) -> int:
        """Return the interpolated 'duty' value based on the given profile and 'temp' value"""
        lower, upper = profile[0], profile[-1]
        for step in profile:
            if step[0] <= temp:
                lower = step
            if step[0] >= temp:
                upper = step
                break
        if lower[0] == upper[0]:
            return lower[1]
        return round(lower[1] + (temp - lower[0]) / (upper[0] - lower[0]) * (upper[1] - lower[1]))

    @staticmethod
    def convert_linespace_to_list(linespace_result: ndarray) -> list[int]:
        return list(map(lambda number: int(number), linespace_result))
