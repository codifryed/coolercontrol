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
from typing import Tuple, List

from liquidctl.util import normalize_profile, interpolate_profile
from numpy import ndarray, asarray, exp, convolve, linspace, ones

from coolercontrol.models.device import DeviceType


class ButtonUtils:

    @staticmethod
    @lru_cache(maxsize=128)
    def extract_info_from_channel_btn_id(channel_btn_id: str) -> Tuple[int, str, DeviceType]:
        """Utility method to extract the parts from the channel_btn_id String
        channel_btn_id looks like: btn_liquidctl_lc-device-id_channel-name"""
        parts = channel_btn_id.split('_')
        device_type_str = str(parts[1])
        if device_type_str == 'hwmon':
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
    def current_value_from_moving_average(values: List[float], window: int, exponential: bool = False) -> float:
        """Compute moving average and return final/current value"""
        np_values = asarray(values)
        weights = exp(linspace(-1., 0., window)) if exponential else ones(window)
        weights /= weights.sum()
        moving_average: ndarray = convolve(np_values, weights, mode='valid')
        return float(moving_average[-1])

    @staticmethod
    def convert_axis_to_profile(temps: List[int], duties: List[int]) -> List[Tuple[int, int]]:
        """Converts two axis to a list of pairs"""
        return list(zip(temps, duties))

    @staticmethod
    def norm_profile(
            profile: List[Tuple[int, int]],
            critical_temp: int,
            max_duty_value: int = 100
    ) -> List[Tuple[int, int]]:
        """Sort, cleanup and set safety levels for the given profile"""
        return normalize_profile(profile, critical_temp, max_duty_value)  # type: ignore[no-any-return]

    @staticmethod
    def interpol_profile(profile: List[Tuple[int, int]], temp: float) -> int:
        """Return the interpolated 'duty' value based on the given profile and 'temp' value"""
        return interpolate_profile(profile, temp)  # type: ignore[no-any-return]

    @staticmethod
    def convert_linespace_to_list(linespace_result: ndarray) -> List[int]:
        return list(map(lambda number: int(number), linespace_result))
