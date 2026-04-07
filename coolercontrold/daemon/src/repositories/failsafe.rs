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

use std::collections::HashMap;
use std::ops::Not;

use crate::device::{ChannelName, ChannelStatus, Mhz, Status, Temp, TempStatus, Watts, RPM};

/// Consecutive missing status readings before failsafe values activate.
pub const MISSING_STATUS_THRESHOLD: usize = 8;
/// Critical high temperature reported when sensor data is missing.
pub const MISSING_TEMP_FAILSAFE: Temp = 100.;
/// Fan duty reported when sensor data is missing.
pub const MISSING_DUTY_FAILSAFE: f64 = 0.;
/// Fan RPM reported when sensor data is missing.
pub const MISSING_RPM_FAILSAFE: RPM = 0;
/// Power draw reported when sensor data is missing.
pub const MISSING_WATTS_FAILSAFE: Watts = 0.;
/// Frequency reported when sensor data is missing.
pub const MISSING_FREQ_FAILSAFE: Mhz = 0;

// Static assertions documenting constant relationships.
const _: () = assert!(MISSING_STATUS_THRESHOLD > 0);
const _: () = assert!(MISSING_RPM_FAILSAFE == 0);
const _: () = assert!(MISSING_FREQ_FAILSAFE == 0);

/// Tracks consecutive missing sensor readings for a single device and
/// holds pre-built failsafe channel/temp values to substitute when the
/// threshold is exceeded.
pub struct FailsafeStatusData {
    pub count: usize,
    pub logged: bool,
    pub channel_failsafes: HashMap<ChannelName, ChannelStatus>,
    pub temp_failsafes: HashMap<ChannelName, TempStatus>,
}

/// Upper bound on the failure counter. Once the threshold is exceeded,
/// further increments serve no purpose and this cap prevents overflow.
const MAX_FAILURE_COUNT: usize = MISSING_STATUS_THRESHOLD + 1;
const _: () = assert!(MAX_FAILURE_COUNT > MISSING_STATUS_THRESHOLD);

impl FailsafeStatusData {
    pub fn new(
        channel_failsafes: HashMap<ChannelName, ChannelStatus>,
        temp_failsafes: HashMap<ChannelName, TempStatus>,
    ) -> Self {
        // At least one failsafe entry is required. A device may have
        // only temps (pure sensor) or only channels (fan-only controller).
        assert!(channel_failsafes.is_empty().not() || temp_failsafes.is_empty().not());
        Self {
            count: 0,
            logged: false,
            channel_failsafes,
            temp_failsafes,
        }
    }

    /// Records a missing status reading. Returns whether the threshold
    /// has been exceeded after this failure.
    pub fn record_failure(&mut self) -> bool {
        if self.count < MAX_FAILURE_COUNT {
            self.count += 1;
        }
        debug_assert!(self.count <= MAX_FAILURE_COUNT);
        self.count > MISSING_STATUS_THRESHOLD
    }

    /// Resets the failure counter on a successful status reading.
    pub fn record_success(&mut self) {
        if self.count > 0 {
            self.count = 0;
        }
    }

    /// Returns whether enough consecutive failures have occurred to
    /// activate failsafe values.
    pub fn threshold_exceeded(&self) -> bool {
        self.count > MISSING_STATUS_THRESHOLD
    }

    /// Marks the error as logged. Returns true if this is the first
    /// time the error is being logged (i.e. was not previously logged).
    pub fn log_once(&mut self) -> bool {
        if self.logged {
            return false;
        }
        self.logged = true;
        true
    }

    /// Builds a complete `Status` from the stored failsafe data.
    pub fn build_failsafe_status(&self) -> Status {
        let channel_count = self.channel_failsafes.len();
        let temp_count = self.temp_failsafes.len();
        let mut channels = Vec::with_capacity(channel_count);
        channels.extend(self.channel_failsafes.values().cloned());
        let mut temps = Vec::with_capacity(temp_count);
        temps.extend(self.temp_failsafes.values().cloned());
        debug_assert_eq!(channels.len(), channel_count);
        debug_assert_eq!(temps.len(), temp_count);
        Status {
            temps,
            channels,
            ..Default::default()
        }
    }
}

/// Creates failsafe channel and temp data from the initial status output
/// of a device. Each channel's optional fields are mapped to their
/// failsafe constant if present, preserving the original structure.
pub fn create_failsafe_data(
    channel_statuses: &[ChannelStatus],
    temp_statuses: &[TempStatus],
) -> (
    HashMap<ChannelName, ChannelStatus>,
    HashMap<ChannelName, TempStatus>,
) {
    let channel_failsafes = channel_statuses
        .iter()
        .map(|s| {
            let status = ChannelStatus {
                name: s.name.clone(),
                rpm: s.rpm.and(Some(MISSING_RPM_FAILSAFE)),
                duty: s.duty.and(Some(MISSING_DUTY_FAILSAFE)),
                freq: s.freq.and(Some(MISSING_FREQ_FAILSAFE)),
                watts: s.watts.and(Some(MISSING_WATTS_FAILSAFE)),
                pwm_mode: s.pwm_mode,
            };
            (s.name.clone(), status)
        })
        .collect();
    let temp_failsafes = temp_statuses
        .iter()
        .map(|t| {
            (
                t.name.clone(),
                TempStatus {
                    name: t.name.clone(),
                    temp: MISSING_TEMP_FAILSAFE,
                },
            )
        })
        .collect();
    (channel_failsafes, temp_failsafes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_channels() -> Vec<ChannelStatus> {
        vec![
            ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                freq: None,
                watts: None,
                pwm_mode: Some(1),
            },
            ChannelStatus {
                name: "power1".to_string(),
                rpm: None,
                duty: None,
                freq: None,
                watts: Some(65.0),
                pwm_mode: None,
            },
        ]
    }

    fn sample_temps() -> Vec<TempStatus> {
        vec![TempStatus {
            name: "temp1".to_string(),
            temp: 45.0,
        }]
    }

    // --- create_failsafe_data: positive space ---

    #[test]
    fn failsafe_data_preserves_channel_structure() {
        // Failsafe channels must keep the same optional field pattern
        // as the original, but with failsafe constant values.
        let (channels, _) = create_failsafe_data(&sample_channels(), &sample_temps());
        assert_eq!(channels.len(), 2);

        let fan = &channels["fan1"];
        assert_eq!(fan.rpm, Some(MISSING_RPM_FAILSAFE));
        assert_eq!(fan.duty, Some(MISSING_DUTY_FAILSAFE));
        assert!(fan.freq.is_none());
        assert!(fan.watts.is_none());
        assert_eq!(fan.pwm_mode, Some(1));

        let power = &channels["power1"];
        assert!(power.rpm.is_none());
        assert!(power.duty.is_none());
        assert_eq!(power.watts, Some(MISSING_WATTS_FAILSAFE));
    }

    #[test]
    fn failsafe_data_sets_critical_temp() {
        // Failsafe temps must be set to the critical high value.
        let (_, temps) = create_failsafe_data(&sample_channels(), &sample_temps());
        assert_eq!(temps.len(), 1);
        let temp = &temps["temp1"];
        assert!((temp.temp - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
    }

    // --- FailsafeStatusData lifecycle ---

    #[test]
    fn new_starts_at_zero_count() {
        // A freshly created tracker must have no failures recorded.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te);
        assert_eq!(fsd.count, 0);
        assert!(fsd.logged.not());
        assert!(fsd.threshold_exceeded().not());
    }

    #[test]
    fn record_failure_increments_count() {
        // Each failure must increment the counter by exactly one.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te);
        for i in 1..=MISSING_STATUS_THRESHOLD {
            let exceeded = fsd.record_failure();
            assert_eq!(fsd.count, i);
            assert!(exceeded.not());
        }
        // One more tips over the threshold.
        let exceeded = fsd.record_failure();
        assert!(exceeded);
        assert!(fsd.threshold_exceeded());
    }

    #[test]
    fn record_success_resets_count() {
        // A successful reading must reset the failure counter.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te);
        for _ in 0..5 {
            fsd.record_failure();
        }
        assert_eq!(fsd.count, 5);
        fsd.record_success();
        assert_eq!(fsd.count, 0);
        assert!(fsd.threshold_exceeded().not());
    }

    #[test]
    fn record_success_noop_at_zero() {
        // Resetting at zero must not underflow or cause issues.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te);
        fsd.record_success();
        assert_eq!(fsd.count, 0);
    }

    #[test]
    fn log_once_returns_true_only_first_time() {
        // The first log_once call must return true, subsequent false.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te);
        assert!(fsd.log_once());
        assert!(fsd.log_once().not());
        assert!(fsd.log_once().not());
    }

    #[test]
    fn build_failsafe_status_contains_all_entries() {
        // The built status must include all failsafe channels and temps.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te);
        let status = fsd.build_failsafe_status();
        assert_eq!(status.channels.len(), 2);
        assert_eq!(status.temps.len(), 1);
    }

    // --- threshold boundary: negative space ---

    #[test]
    fn threshold_not_exceeded_at_exact_boundary() {
        // At exactly MISSING_STATUS_THRESHOLD failures, the threshold
        // must NOT be exceeded (it requires strictly greater than).
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te);
        for _ in 0..MISSING_STATUS_THRESHOLD {
            fsd.record_failure();
        }
        assert_eq!(fsd.count, MISSING_STATUS_THRESHOLD);
        assert!(fsd.threshold_exceeded().not());
    }

    #[test]
    fn threshold_exceeded_one_past_boundary() {
        // One failure past the threshold must activate failsafe.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te);
        for _ in 0..=MISSING_STATUS_THRESHOLD {
            fsd.record_failure();
        }
        assert_eq!(fsd.count, MISSING_STATUS_THRESHOLD + 1);
        assert!(fsd.threshold_exceeded());
    }

    #[test]
    fn record_failure_caps_at_max() {
        // The counter must not grow beyond MAX_FAILURE_COUNT to
        // prevent theoretical overflow from unbounded increment.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te);
        for _ in 0..1000 {
            fsd.record_failure();
        }
        assert_eq!(fsd.count, super::MAX_FAILURE_COUNT);
        assert!(fsd.threshold_exceeded());
    }

    #[test]
    fn build_failsafe_status_has_correct_values() {
        // The built status must contain the actual failsafe constant
        // values, not the original device readings.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te);
        let status = fsd.build_failsafe_status();
        for temp in &status.temps {
            assert!((temp.temp - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
        }
        for channel in &status.channels {
            if let Some(duty) = channel.duty {
                assert!((duty - MISSING_DUTY_FAILSAFE).abs() < f64::EPSILON);
            }
            if let Some(rpm) = channel.rpm {
                assert_eq!(rpm, MISSING_RPM_FAILSAFE);
            }
        }
    }

    use std::ops::Not;
}
