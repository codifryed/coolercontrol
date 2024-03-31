/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::collections::VecDeque;

/// Sort, cleanup, and set safety levels for the given profile[(temp, duty)].
/// This will ensure that:
///   - the profile is a monotonically increasing function
///   - the profile is sorted
///   - a (`critical_temp`, 100%) failsafe is enforced
///   - only the first profile step with duty=100% is kept
pub fn normalize_profile(
    profile: &[(f64, u8)],
    critical_temp: f64,
    max_duty_value: u8,
) -> Vec<(f64, u8)> {
    let mut sorted_profile: VecDeque<(f64, u8)> = profile.iter().copied().collect();
    sorted_profile.push_back((critical_temp, max_duty_value));
    sorted_profile
        .make_contiguous()
        .sort_by(|(temp_a, duty_a), (temp_b, duty_b)|
            // reverse ordering for duty so that the largest given duty value is used
            temp_a.partial_cmp(temp_b).unwrap().then(duty_b.cmp(duty_a)));
    let mut normalized_profile = Vec::new();
    normalized_profile.push(sorted_profile.pop_front().unwrap());
    let (mut previous_temp, mut previous_duty) = normalized_profile[0];
    for (temp, duty) in sorted_profile {
        if temp == previous_temp {
            continue; // skip duplicate temps
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
pub fn interpolate_profile(normalized_profile: &[(f64, u8)], temp: f64) -> u8 {
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
        return step_below.1; // temp matches exactly, no duty calculation needed
    }
    let (step_below_temp, step_below_duty) = (step_below.0, f64::from(step_below.1));
    let (step_above_temp, step_above_duty) = (step_above.0, f64::from(step_above.1));
    (step_below_duty
        + (temp - step_below_temp) / (step_above_temp - step_below_temp)
            * (step_above_duty - step_below_duty))
        .round() as u8
}

#[cfg(test)]
mod tests {
    use crate::processing::utils::{interpolate_profile, normalize_profile};

    #[test]
    fn normalize_profile_test() {
        let given_expected = vec![
            (
                (
                    vec![
                        (30f64, 40u8),
                        (25.0, 25),
                        (35.0, 30),
                        (40.0, 35),
                        (40.0, 80),
                    ],
                    60f64,
                    100,
                ),
                vec![
                    (25f64, 25u8),
                    (30.0, 40),
                    (35.0, 40),
                    (40.0, 80),
                    (60.0, 100),
                ],
            ),
            (
                (
                    vec![(30.0, 40), (25.0, 25), (35.0, 30), (40.0, 100)],
                    60.0,
                    100,
                ),
                vec![(25.0, 25), (30.0, 40), (35.0, 40), (40.0, 100)],
            ),
            (
                (
                    vec![(30.0, 40), (25.0, 25), (35.0, 100), (40.0, 100)],
                    60.0,
                    100,
                ),
                vec![(25.0, 25), (30.0, 40), (35.0, 100)],
            ),
            ((vec![], 60.0, 100), vec![(60.0, 100)]),
            ((vec![], 60.0, 200), vec![(60.0, 200)]),
        ];

        for (given, expected) in given_expected {
            assert_eq!(normalize_profile(&given.0, given.1, given.2), expected);
        }
    }

    #[test]
    fn interpolate_profile_test() {
        let given_expected = vec![
            ((vec![(20f64, 50u8), (50.0, 70), (60.0, 100)], 33.), 59u8),
            ((vec![(20.0, 50), (50.0, 70)], 19.), 50),
            ((vec![(20.0, 50), (50.0, 70)], 51.), 70),
            ((vec![(20.0, 50)], 20.), 50),
        ];
        for (given, expected) in given_expected {
            assert_eq!(interpolate_profile(&given.0, given.1), expected);
        }
    }
}
