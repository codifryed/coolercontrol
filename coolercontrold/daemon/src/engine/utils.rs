/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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

use crate::device::{Duty, Temp};
use crate::setting::Offset;
use std::collections::VecDeque;

/// Sort, cleanup, and set safety levels for the given profile[(temp, duty)].
/// This will ensure that:
///   - the profile is a monotonically increasing function
///   - the profile is sorted
///   - a (`critical_temp`, 100%) failsafe is enforced
///   - only the first profile step with duty=100% is kept
#[allow(clippy::float_cmp)]
pub fn normalize_profile(
    profile: &[(Temp, Duty)],
    critical_temp: Temp,
    max_duty_value: Duty,
) -> Vec<(Temp, Duty)> {
    let mut sorted_profile: VecDeque<(Temp, Duty)> = profile.iter().copied().collect();
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
        // strict comparison should be fine here, as we're comparing its own previous value:
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
/// This will ensure that:
///   - the profile duty is a monotonically increasing function
///   - the profile is sorted
///   - offset profile offsets may go up or down, so no failsafe is enforced
pub fn normalize_offset_profile(profile: &[(Duty, Offset)]) -> Vec<(Duty, Offset)> {
    let mut sorted_profile: VecDeque<(Duty, Offset)> = profile.iter().copied().collect();
    sorted_profile
        .make_contiguous()
        .sort_by(|(duty_a, offset_a), (duty_b, offset_b)|
            // reverse ordering for offset so that the largest given offset value is used
            duty_a.partial_cmp(duty_b).unwrap().then(offset_b.cmp(offset_a)));
    for (_, offset) in &mut sorted_profile {
        // clamp offsets to limits
        *offset = *(offset.clamp(&mut -100, &mut 100));
    }
    let mut normalized_profile = Vec::new();
    normalized_profile.push(sorted_profile.pop_front().unwrap());
    let mut previous_duty = normalized_profile[0].0;
    for (duty, offset) in sorted_profile {
        if duty == previous_duty {
            continue; // skip duplicate duties
        }
        normalized_profile.push((duty, offset));
        if duty == 100 {
            break;
        }
        previous_duty = duty;
    }
    normalized_profile
}

/// Interpolate duty for a given temp and profile(temp, duty).
/// profile must be normalized first for this function to work as expected (Temp always increasing).
/// Returned duty is rounded to the nearest integer.
///
/// This is a custom interpolation function that is designed for our use case.
/// Uses binary search for O(log n) lookup instead of linear scan.
#[allow(
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn interpolate_profile(normalized_profile: &[(Temp, Duty)], temp: Temp) -> Duty {
    // Handle edge cases
    if normalized_profile.is_empty() {
        return 0;
    }
    if normalized_profile.len() == 1 {
        return normalized_profile[0].1;
    }

    // Binary search for the insertion point
    let idx = normalized_profile
        .binary_search_by(|(t, _)| t.partial_cmp(&temp).unwrap())
        .unwrap_or_else(|i| i);

    // Clamp to valid range
    if idx == 0 {
        return normalized_profile[0].1;
    }
    if idx >= normalized_profile.len() {
        return normalized_profile.last().unwrap().1;
    }

    // Interpolate between idx-1 and idx
    let (step_below_temp, step_below_duty) = normalized_profile[idx - 1];
    let (step_above_temp, step_above_duty) = normalized_profile[idx];

    if step_below_temp == step_above_temp {
        return step_below_duty; // temp matches exactly, no duty calculation needed
    }

    let step_below_duty = f64::from(step_below_duty);
    let step_above_duty = f64::from(step_above_duty);
    (step_below_duty
        + (temp - step_below_temp) / (step_above_temp - step_below_temp)
            * (step_above_duty - step_below_duty))
        .round() as u8
}

/// Interpolate offset for a given duty and profile(duty, offset).
/// profile must be normalized first for this function to work as expected.
/// Returned offset is rounded to the nearest integer.
///
/// This is a custom interpolation function that is designed for our use case.
/// Uses binary search for O(log n) lookup instead of linear scan.
#[allow(clippy::cast_possible_truncation)]
pub fn interpolate_offset_profile(normalized_profile: &[(Duty, Offset)], duty: Duty) -> Offset {
    // Handle edge cases
    if normalized_profile.is_empty() {
        return 0;
    }
    if normalized_profile.len() == 1 {
        return normalized_profile[0].1;
    }

    // Binary search for the insertion point
    let idx = normalized_profile
        .binary_search_by(|(d, _)| d.cmp(&duty))
        .unwrap_or_else(|i| i);

    // Clamp to valid range
    if idx == 0 {
        return normalized_profile[0].1;
    }
    if idx >= normalized_profile.len() {
        return normalized_profile.last().unwrap().1;
    }

    // Interpolate between idx-1 and idx
    let (step_below_duty, step_below_offset) = normalized_profile[idx - 1];
    let (step_above_duty, step_above_offset) = normalized_profile[idx];

    if step_below_duty == step_above_duty {
        return step_below_offset; // duty matches exactly, no offset calculation needed
    }

    let step_below_duty = f64::from(step_below_duty);
    let step_below_offset = f64::from(step_below_offset);
    let step_above_duty = f64::from(step_above_duty);
    let step_above_offset = f64::from(step_above_offset);
    (step_below_offset
        + (f64::from(duty) - step_below_duty) / (step_above_duty - step_below_duty)
            * (step_above_offset - step_below_offset))
        .round() as Offset
}

#[cfg(test)]
mod tests {
    use crate::engine::utils::{
        interpolate_offset_profile, interpolate_profile, normalize_offset_profile,
        normalize_profile,
    };

    #[test]
    fn test_normalize_profile() {
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
            ((vec![(25.0, 30)], 60.0, 100), vec![(25.0, 30), (60.0, 100)]),
            ((vec![], 60.0, 100), vec![(60.0, 100)]),
            ((vec![], 60.0, 200), vec![(60.0, 200)]),
        ];

        for (given, expected) in given_expected {
            assert_eq!(normalize_profile(&given.0, given.1, given.2), expected);
        }
    }

    #[test]
    fn test_interpolate_profile() {
        let given_expected = vec![
            ((vec![(20f64, 50u8), (50.0, 70), (60.0, 100)], 33.), 59u8),
            ((vec![(20.0, 50), (50.0, 70)], 19.), 50),
            ((vec![(20.0, 50), (50.0, 70)], 51.), 70),
        ];
        for (given, expected) in given_expected {
            assert_eq!(interpolate_profile(&given.0, given.1), expected);
        }
    }

    #[test]
    fn test_interpolate_negative_profile() {
        let given_expected = vec![
            ((vec![(20f64, 50u8), (50.0, 70), (60.0, 20)], 33.), 59u8),
            ((vec![(10.0, 50), (20.0, 30), (30.0, 80)], 20.), 30),
            ((vec![(20.0, 50), (50.0, 30)], 51.), 30),
        ];
        for (given, expected) in given_expected {
            assert_eq!(interpolate_profile(&given.0, given.1), expected);
        }
    }

    #[test]
    fn test_interpolate_single_profile() {
        let given_expected = vec![
            ((vec![(20.0, 50)], 20.), 50),
            ((vec![(20.0, 50)], 0.), 50),
            ((vec![(20.0, 50)], 80.), 50),
            ((vec![(20.0, 70)], 80.), 70),
            ((vec![(20.0, 10)], 80.), 10),
        ];
        for (given, expected) in given_expected {
            assert_eq!(interpolate_profile(&given.0, given.1), expected);
        }
    }

    #[test]
    fn test_normalize_offset_profile() {
        let given_expected = vec![
            (
                // can go back down and up, and double duty values are handled
                vec![(30u8, 40i8), (25, 25), (35, 30), (40, 35), (40, 80)],
                vec![(25u8, 25i8), (30, 40), (35, 30), (40, 80)],
            ),
            (
                vec![(30, 40), (25, 25), (35, 30), (40, 100)],
                vec![(25, 25), (30, 40), (35, 30), (40, 100)],
            ),
            (
                vec![(30, 40), (25, 25), (35, 100), (40, 100)],
                vec![(25, 25), (30, 40), (35, 100), (40, 100)],
            ),
            (
                // make sure offsets are clamped
                vec![(30, 40), (25, -120), (35, 120), (40, i8::MAX)],
                vec![(25, -100), (30, 40), (35, 100), (40, 100)],
            ),
            (vec![(25, 30)], vec![(25, 30)]),
        ];

        for (given, expected) in given_expected {
            assert_eq!(normalize_offset_profile(&given), expected);
        }
    }

    #[test]
    fn test_interpolate_offset_profile() {
        let given_expected = vec![
            ((vec![(20u8, 50i8), (50, 70), (60, 100)], 33), 59i8),
            ((vec![(20, 50), (50, 70)], 19), 50),
            ((vec![(20, 50), (50, 70)], 51), 70),
        ];
        for (given, expected) in given_expected {
            assert_eq!(interpolate_offset_profile(&given.0, given.1), expected);
        }
    }

    #[test]
    fn test_interpolate_negative_offset_profile() {
        let given_expected = vec![
            ((vec![(20u8, 50i8), (50, 70), (60, 20)], 33), 59i8),
            ((vec![(10, 50), (20, 30), (30, 80)], 20), 30),
            ((vec![(20, 50), (50, 30)], 51), 30),
        ];
        for (given, expected) in given_expected {
            assert_eq!(interpolate_offset_profile(&given.0, given.1), expected);
        }
    }

    #[test]
    fn test_interpolate_single_offset_profile() {
        let given_expected = vec![
            ((vec![(20, 50)], 20), 50),
            ((vec![(20, 50)], 0), 50),
            ((vec![(20, 50)], 80), 50),
            ((vec![(20, 70)], 80), 70),
            ((vec![(20, 10)], 80), 10),
        ];
        for (given, expected) in given_expected {
            assert_eq!(interpolate_offset_profile(&given.0, given.1), expected);
        }
    }

    // #[bench]
    // fn bench_interpolate_profile(b: &mut test::Bencher) {
    //     let given = (
    //         vec![(20f64, 50u8), (50.0, 70), (60.0, 100)],
    //         33.,
    //     );
    //     b.iter(|| black_box(interpolate_profile(&given.0, given.1)));
    // }
}
