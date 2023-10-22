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


use std::collections::{HashMap, VecDeque};
use std::ops::Add;
use std::process::Stdio;
use std::time::{Duration, Instant};

use log::error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::sleep;
use yata::methods::{SMA, TMA};
use yata::prelude::Method;

const TMA_WINDOW_SIZE: u8 = 8;
pub const SMA_WINDOW_SIZE: u8 = 3;
pub const SAMPLE_SIZE: isize = 16;

/// Sort, cleanup, and set safety levels for the given profile[(temp, duty)].
/// This will ensure that:
///   - the profile is a monotonically increasing function
///   - the profile is sorted
///   - a (critical_temp, 100%) failsafe is enforced
///   - only the first profile step with duty=100% is kept
pub fn normalize_profile(profile: &[(f64, u8)], critical_temp: f64, max_duty_value: u8) -> Vec<(f64, u8)> {
    let mut sorted_profile: VecDeque<(f64, u8)> = profile.iter().copied().collect();
    sorted_profile.push_back((critical_temp, max_duty_value));
    sorted_profile.make_contiguous().sort_by(
        |(temp_a, duty_a), (temp_b, duty_b)|
            // reverse ordering for duty so that the largest given duty value is used
            temp_a.partial_cmp(temp_b).unwrap().then(duty_b.cmp(duty_a))
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

/// Computes a simple moving average from give values and returns the those averages.
/// Simple is just a moving average with no weight. This is particularly helpful for graphing
/// dynamic temperature sources like GPU as the constant fluctuations are smoothed out and the recent
/// temp values won't change over time, unlike with exponential moving averages.
/// Rounded to the nearest 100th decimal place
pub fn all_values_from_simple_moving_average(all_values: &[f64], window_multiplier: u8) -> Vec<f64> {
    SMA::new_over(SMA_WINDOW_SIZE * window_multiplier, all_values).unwrap().iter()
        .map(|temp| (temp * 100.).round() / 100.)
        .collect()
}

/// Computes an exponential moving average from give temps and returns the final/current value from that average.
/// Exponential moving average gives the most recent values more weight. This is particularly helpful
/// for setting duty for dynamic temperature sources like CPU. (Good reaction but also averaging)
/// Will panic if sample_size is 0.
/// Rounded to the nearest 100th decimal place
pub fn current_temp_from_exponential_moving_average(all_temps: &[f64]) -> f64 {
    (TMA::new_over(TMA_WINDOW_SIZE, get_temps_slice(all_temps)).unwrap()
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

/// This struct is essentially a wrapper around [`tokio::process::Command`] which adds some
/// additional safety measures and handling for our use cases.
pub struct ShellCommand {
    command: String,
    timeout: Duration,
    env: HashMap<String, String>,
}

pub enum ShellCommandResult {
    Success { stdout: String, stderr: String },
    Error(String),
}

impl ShellCommand {
    pub fn new(command: &str, timeout: Duration) -> Self {
        Self {
            command: command.to_owned(),
            timeout,
            env: HashMap::new(),
        }
    }

    pub fn env(&mut self, key: &str, value: &str) -> &mut Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }

    pub async fn run(&self) -> ShellCommandResult {
        let mut successful = false;
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut shell_command = Command::new("sh");
        shell_command
            .arg("-c")
            .arg(&self.command)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in self.env.iter() {
            shell_command.env(key, value);
        }
        let spawned_process = shell_command.spawn();
        let timeout_time = Instant::now().add(self.timeout);
        match spawned_process {
            Ok(mut child) => {
                while Instant::now() < timeout_time {
                    sleep(Duration::from_millis(50)).await;
                    if let Some(_) = child.try_wait().unwrap() {
                        break;
                    }
                }
                successful = match child.try_wait().unwrap() {
                    None => {
                        error!(
                                "Shell command did not complete within the specified timeout: {:?} \
                                Killing process for: {}",
                                self.timeout,
                                self.command
                            );
                        child.kill().await.ok();
                        child.wait().await.ok().unwrap().success()
                    }
                    Some(status) => status.success()
                };
                if let Some(mut child_err) = child.stderr.take() {
                    child_err.read_to_string(&mut stderr).await.unwrap();
                    stderr = stderr.trim().to_owned();
                };
                if let Some(mut child_out) = child.stdout.take() {
                    child_out.read_to_string(&mut stdout).await.unwrap();
                    stdout = stdout.trim().to_owned();
                }
            }
            Err(err) => {
                error!("Unexpected Error spawning process for command: {}, {}", &self.command, err);
                stderr = err.to_string();
            }
        }
        if successful {
            ShellCommandResult::Success { stdout, stderr }
        } else {
            ShellCommandResult::Error(stderr)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::{all_values_from_simple_moving_average, current_temp_from_exponential_moving_average, interpolate_profile, normalize_profile};

    #[test]
    fn normalize_profile_test() {
        let given_expected = vec![
            (
                (vec![(30f64, 40u8), (25.0, 25), (35.0, 30), (40.0, 35), (40.0, 80)], 60f64, 100),
                vec![(25f64, 25u8), (30.0, 40), (35.0, 40), (40.0, 80), (60.0, 100)]
            ),
            (
                (vec![(30.0, 40), (25.0, 25), (35.0, 30), (40.0, 100)], 60.0, 100),
                vec![(25.0, 25), (30.0, 40), (35.0, 40), (40.0, 100)]
            ),
            (
                (vec![(30.0, 40), (25.0, 25), (35.0, 100), (40.0, 100)], 60.0, 100),
                vec![(25.0, 25), (30.0, 40), (35.0, 100)]
            ),
            (
                (vec![], 60.0, 100),
                vec![(60.0, 100)]
            ),
            (
                (vec![], 60.0, 200),
                vec![(60.0, 200)]
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
                (vec![(20f64, 50u8), (50.0, 70), (60.0, 100)], 33.),
                59u8
            ),
            (
                (vec![(20.0, 50), (50.0, 70)], 19.),
                50
            ),
            (
                (vec![(20.0, 50), (50.0, 70)], 51.),
                70
            ),
            (
                (vec![(20.0, 50)], 20.),
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
            // these are just samples. Tested with real hardware for expected results,
            // which are not so clear in numbers here.
            (
                &[20., 25.],
                20.05
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30., 30., 30.],
                35.86
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
        let given_expected: Vec<(&[f64], &[f64])> = vec![
            (
                &[20., 25.],
                &[20.0, 21.67]
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30., 30., 30.],
                &[20.0, 21.67, 25.0, 48.33, 70.0, 90.0, 70.0, 50.0, 30.0, 30.0]
            ),
            (
                &[30., 30., 30., 30.],
                &[30., 30., 30., 30.]
            ),
        ];
        for (given, expected) in given_expected {
            assert_eq!(
                all_values_from_simple_moving_average(given, 1).as_slice(),
                expected
            )
        }
    }
}
