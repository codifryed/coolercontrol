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

from typing import Tuple, List

import liquidctl
import numpy as np
from numpy import ndarray


class ButtonUtils:

    @staticmethod
    def extract_info_from_channel_btn_id(channel_btn_id: str) -> Tuple[int, str]:
        """Utility method to extract the parts from the channel_btn_id String
        channel_btn_id looks like: btn_liquidctl_lc-device-id_channel-name"""
        parts = channel_btn_id.split('_')
        lc_device_id = int(parts[2])
        channel_name = str(parts[3])
        return lc_device_id, channel_name


class MathUtils:

    @staticmethod
    def current_value_from_moving_average(values: List[float], window: int, exponential: bool = False) -> float:
        """Compute moving average and return final/current value"""
        np_values = np.asarray(values)
        weights = np.exp(np.linspace(-1., 0., window)) if exponential else np.ones(window)
        weights /= weights.sum()
        moving_average: ndarray = np.convolve(np_values, weights, mode='valid')
        return float(moving_average[-1])

    @staticmethod
    def convert_axis_to_profile(temps: List[int], duties: List[int]) -> List[Tuple[int, int]]:
        """Converts two axis to a list of pairs"""
        return list(zip(temps, duties))

    @staticmethod
    def normalize_profile(
            profile: List[Tuple[int, int]],
            critical_temp: int,
            max_duty_value: int = 100
    ) -> List[Tuple[int, int]]:
        """Sort, cleanup and set safety levels for the given profile"""
        return liquidctl.util.normalize_profile(profile, critical_temp, max_duty_value)  # type: ignore[no-any-return]

    @staticmethod
    def interpolate_profile(profile: List[Tuple[int, int]], temp: float) -> int:
        """Return the interpolated 'duty' value based on the given profile and 'temp' value"""
        return liquidctl.util.interpolate_profile(profile, temp)  # type: ignore[no-any-return]
