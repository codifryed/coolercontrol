/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use yata::methods::SMA;
use yata::prelude::Method;

pub const SMA_WINDOW_SIZE: u8 = 3;

/// Computes a simple moving average from give values and returns the those averages.
/// Simple is just a moving average with no weight. This is particularly helpful for graphing
/// dynamic temperature sources like GPU as the constant fluctuations are smoothed out and the recent
/// temp values won't change over time, unlike with exponential moving averages.
/// Rounded to the nearest 100th decimal place
pub fn all_values_from_simple_moving_average(
    all_values: &[f64],
    window_multiplier: u8,
) -> Vec<f64> {
    SMA::new_over(SMA_WINDOW_SIZE * window_multiplier, all_values)
        .unwrap()
        .iter()
        .map(|temp| (temp * 100.).round() / 100.)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::api::utils::all_values_from_simple_moving_average;

    #[test]
    fn current_temp_from_simple_moving_average_test() {
        let given_expected: Vec<(&[f64], &[f64])> = vec![
            (&[20., 25.], &[20.0, 21.67]),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30., 30., 30.],
                &[20.0, 21.67, 25.0, 48.33, 70.0, 90.0, 70.0, 50.0, 30.0, 30.0],
            ),
            (&[30., 30., 30., 30.], &[30., 30., 30., 30.]),
        ];
        for (given, expected) in given_expected {
            assert_eq!(
                all_values_from_simple_moving_average(given, 1).as_slice(),
                expected
            )
        }
    }
}
