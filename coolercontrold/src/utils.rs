/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2022  Guy Boldon
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
 ******************************************************************************/


use std::collections::VecDeque;

use yata::methods::{EMA, SMA};
use yata::prelude::Method;

pub const WINDOW_SIZE: u8 = 2; // 2 tested has good dynamic results

pub const SAMPLE_SIZE: isize = 4; // 4 sec. (4 samples of same temp to equal that temp 100%)

/// Sort, cleanup, and set safety levels for the given profile[(temp, duty)].
/// This will ensure that:
///   - the profile is a monotonically increasing function
///   - the profile is sorted
///   - a (critical_temp, 100%) failsafe is enforced
///   - only the first profile step with duty=100% is kept
pub fn normalize_profile(profile: &[(u8, u8)], critical_temp: u8, max_duty_value: u8) -> Vec<(u8, u8)> {
    let mut sorted_profile: VecDeque<(u8, u8)> = profile.iter().copied().collect();
    sorted_profile.push_back((critical_temp, max_duty_value));
    sorted_profile.make_contiguous().sort_by(
        |(temp_a, duty_a), (temp_b, duty_b)|
            // reverse ordering for duty so that the largest given duty value is used
            temp_a.cmp(temp_b).then(duty_b.cmp(duty_a))
    );
    let mut normalized_profile = Vec::new();
    normalized_profile.push(sorted_profile.pop_front().unwrap());
    let (mut previous_temp, mut previous_duty) = normalized_profile[0];
    for (temp, duty) in sorted_profile {
        if temp == previous_temp {
            continue;  // skip duplicate temps
        }
        let adjusted_duty = if duty < previous_duty {
            previous_duty // following duties are not allowed to decrease.
        } else if duty > max_duty_value {
            max_duty_value
        } else {
            duty
        };
        normalized_profile.push((temp, adjusted_duty));
        if adjusted_duty == max_duty_value {
            break;
        }
        previous_temp = temp;
        previous_duty = adjusted_duty;
    }
    normalized_profile
}

/// Interpolate duty from a given temp and profile(temp, duty)
/// profile must be normalized first for this function to work as expected
/// Returned duty is rounded to the nearest integer
pub fn interpolate_profile(normalized_profile: &[(u8, u8)], temp_f64: f64) -> u8 {
    let temp = temp_f64.round() as u8;
    let mut step_below = &normalized_profile[0];
    let mut step_above = normalized_profile.last().unwrap();
    for step in normalized_profile {
        if step.0 <= temp {
            step_below = step;
        }
        if step.0 >= temp {
            step_above = step;
            break;
        }
    }
    if step_below.0 == step_above.0 {
        return step_below.1;  // temp matches exactly, no duty calculation needed
    }
    let (step_below_temp, step_below_duty) = (step_below.0 as f64, step_below.1 as f64);
    let (step_above_temp, step_above_duty) = (step_above.0 as f64, step_above.1 as f64);
    (step_below_duty
        + (temp as f64 - step_below_temp)
        / (step_above_temp - step_below_temp)
        * (step_above_duty - step_below_duty)
    ).round() as u8
}

/// Computes a simple moving average from give temps and returns the final/current value from that average.
/// Simple is just a moving average with no weight. This is particularly helpful for graphing
/// dynamic temperature sources like GPU as the constant fluctuations are smoothed out and the recent
/// temp values won't change over time, unlike with exponential moving averages.
/// Will panic if sample_size is 0.
/// Rounded to the nearest 100th decimal place
pub fn current_temp_from_simple_moving_average(all_temps: &[f64]) -> f64 {
    // SMA is much simpler, in that it doesn't take anything outside of the window_size into consideration
    (SMA::new_over(WINDOW_SIZE, get_temps_slice(all_temps)).unwrap()
        .last().unwrap() * 100.
    ).round() / 100.
}

/// Computes a simple moving average from give values and returns the those averages.
/// Simple is just a moving average with no weight. This is particularly helpful for graphing
/// dynamic temperature sources like GPU as the constant fluctuations are smoothed out and the recent
/// temp values won't change over time, unlike with exponential moving averages.
/// Rounded to the nearest 100th decimal place
pub fn all_values_from_simple_moving_average(all_values: &[f64], window_multiplier: u8) -> Vec<f64> {
    SMA::new_over(WINDOW_SIZE * window_multiplier, all_values).unwrap().iter()
        .map(| temp | (temp * 100.).round() / 100.)
        .collect()
}

/// Computes an exponential moving average from give temps and returns the final/current value from that average.
/// Exponential moving average gives the most recent values more weight. This is particularly helpful
/// for setting duty for dynamic temperature sources like CPU. (Good reaction but also averaging)
/// Will panic if sample_size is 0.
/// Rounded to the nearest 100th decimal place
pub fn current_temp_from_exponential_moving_average(all_temps: &[f64]) -> f64 {
    (EMA::new_over(WINDOW_SIZE, get_temps_slice(all_temps)).unwrap()
        .last().unwrap() * 100.
    ).round() / 100.
}

fn get_temps_slice(all_temps: &[f64]) -> &[f64] {
    // keeping the sample size low allows the average to be more aggressive,
    // otherwise the actual reading and the EMA take quite a while before they are the same value
    let sample_delta = all_temps.len() as isize - SAMPLE_SIZE;
    if sample_delta > 0 {
        all_temps.split_at(sample_delta as usize).1
    } else {
        all_temps
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::{current_temp_from_exponential_moving_average, current_temp_from_simple_moving_average, interpolate_profile, normalize_profile};

    #[test]
    fn normalize_profile_test() {
        let given_expected = vec![
            (
                (vec![(30u8, 40u8), (25, 25), (35, 30), (40, 35), (40, 80)], 60, 100),
                vec![(25u8, 25u8), (30, 40), (35, 40), (40, 80), (60, 100)]
            ),
            (
                (vec![(30, 40), (25, 25), (35, 30), (40, 100)], 60, 100),
                vec![(25, 25), (30, 40), (35, 40), (40, 100)]
            ),
            (
                (vec![(30, 40), (25, 25), (35, 100), (40, 100)], 60, 100),
                vec![(25, 25), (30, 40), (35, 100)]
            ),
            (
                (vec![], 60, 100),
                vec![(60, 100)]
            ),
            (
                (vec![], 60, 200),
                vec![(60, 200)]
            ),
        ];

        for (given, expected) in given_expected {
            assert_eq!(
                normalize_profile(&given.0, given.1, given.2),
                expected
            )
        }
    }

    #[test]
    fn interpolate_profile_test() {
        let given_expected = vec![
            (
                (vec![(20u8, 50u8), (50, 70), (60, 100)], 33.),
                59u8
            ),
            (
                (vec![(20, 50), (50, 70)], 19.),
                50
            ),
            (
                (vec![(20, 50), (50, 70)], 51.),
                70
            ),
            (
                (vec![(20, 50)], 20.),
                50
            ),
        ];
        for (given, expected) in given_expected {
            assert_eq!(
                interpolate_profile(&given.0, given.1),
                expected
            )
        }
    }

    #[test]
    fn current_temp_from_exponential_moving_average_test() {
        let given_expected: Vec<(&[f64], f64)> = vec![
            (
                &[20., 25.],
                23.33
            ),
            (
                &[20., 25., 30.],
                27.78
            ),
            (
                &[20., 25., 30., 90.],
                69.26
            ),
            (
                &[20., 25., 30., 90., 90.],
                83.15
            ),
            (
                &[20., 25., 30., 90., 90., 90.],
                87.78
            ),
            (
                &[20., 25., 30., 90., 90., 90., 90.],
                90.
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30.],
                50.
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30.],
                36.67
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30., 30.],
                32.22
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30., 30., 30.],
                30.
            ),
            (
                &[30., 30., 30., 30.],
                30.
            ),
        ];
        for (given, expected) in given_expected {
            assert_eq!(
                current_temp_from_exponential_moving_average(given),
                expected
            )
        }
    }

    #[test]
    fn current_temp_from_simple_moving_average_test() {
        let given_expected: Vec<(&[f64], f64)> = vec![
            (
                &[20., 25.],
                22.5
            ),
            (
                &[20., 25., 30.],
                27.5
            ),
            (
                &[20., 25., 30., 90.],
                60.
            ),
            (
                &[20., 25., 30., 90., 90.],
                90.
            ),
            (
                &[20., 25., 30., 90., 90., 90.],
                90.
            ),
            (
                &[20., 25., 30., 90., 90., 90., 90.],
                90.
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30.],
                60.
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30.],
                30.
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30., 30.],
                30.
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30., 30., 30.],
                30.
            ),
            (
                &[30., 30., 30., 30.],
                30.
            ),
        ];
        for (given, expected) in given_expected {
            assert_eq!(
                current_temp_from_simple_moving_average(given),
                expected
            )
        }
    }
}
