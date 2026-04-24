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

use std::collections::{HashMap, HashSet};

use crate::device::{ChannelName, ChannelStatus, Mhz, Status, Temp, TempStatus, Watts, RPM};

/// Consecutive missing status readings before failsafe values activate.
/// A little more than the max timeout for waiting to write values to allow recovery time.
pub const MISSING_STATUS_THRESHOLD: usize = 10;
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
    /// Returns `None` when both maps are empty, meaning the device
    /// has no status data to protect (e.g. read paths not yet available).
    pub fn new(
        channel_failsafes: HashMap<ChannelName, ChannelStatus>,
        temp_failsafes: HashMap<ChannelName, TempStatus>,
    ) -> Option<Self> {
        if channel_failsafes.is_empty() && temp_failsafes.is_empty() {
            return None;
        }
        Some(Self {
            count: 0,
            logged: false,
            channel_failsafes,
            temp_failsafes,
        })
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

    /// Overwrites cache entries with failsafe values for every expected
    /// channel / temp name that is absent from this tick's fresh read.
    /// Entries whose names are in `fresh_*_names` are left untouched so
    /// working sensors keep serving real values. A defensive push is
    /// used for expected names that are not already in the cache.
    pub fn overwrite_missing(
        &self,
        channels: &mut Vec<ChannelStatus>,
        temps: &mut Vec<TempStatus>,
        fresh_channel_names: &HashSet<&str>,
        fresh_temp_names: &HashSet<&str>,
    ) {
        for (name, failsafe_channel) in &self.channel_failsafes {
            if fresh_channel_names.contains(name.as_str()) {
                continue;
            }
            if let Some(entry) = channels.iter_mut().find(|c| &c.name == name) {
                *entry = failsafe_channel.clone();
            } else {
                channels.push(failsafe_channel.clone());
            }
        }
        for (name, failsafe_temp) in &self.temp_failsafes {
            if fresh_temp_names.contains(name.as_str()) {
                continue;
            }
            if let Some(entry) = temps.iter_mut().find(|t| &t.name == name) {
                *entry = failsafe_temp.clone();
            } else {
                temps.push(failsafe_temp.clone());
            }
        }
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
    use std::ops::Not;

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
    fn new_returns_none_when_both_maps_empty() {
        // A device with no channels and no temps has nothing to protect.
        let result = FailsafeStatusData::new(HashMap::new(), HashMap::new());
        assert!(result.is_none());
    }

    #[test]
    fn new_starts_at_zero_count() {
        // A freshly created tracker must have no failures recorded.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te).unwrap();
        assert_eq!(fsd.count, 0);
        assert!(fsd.logged.not());
        assert!(fsd.threshold_exceeded().not());
    }

    #[test]
    fn record_failure_increments_count() {
        // Each failure must increment the counter by exactly one.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
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
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
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
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
        fsd.record_success();
        assert_eq!(fsd.count, 0);
    }

    #[test]
    fn log_once_returns_true_only_first_time() {
        // The first log_once call must return true, subsequent false.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
        assert!(fsd.log_once());
        assert!(fsd.log_once().not());
        assert!(fsd.log_once().not());
    }

    #[test]
    fn build_failsafe_status_contains_all_entries() {
        // The built status must include all failsafe channels and temps.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te).unwrap();
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
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
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
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
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
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
        for _ in 0..1000 {
            fsd.record_failure();
        }
        assert_eq!(fsd.count, super::MAX_FAILURE_COUNT);
        assert!(fsd.threshold_exceeded());
    }

    // --- overwrite_missing: cache-preserving failsafe substitution ---

    #[test]
    fn overwrite_missing_no_op_when_all_fresh_names_present() {
        // When every expected name appeared in this tick's fresh read,
        // the cache must remain untouched. The fresh set is the
        // authoritative indicator of "this channel reported this tick".
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te).unwrap();
        let mut channels = vec![
            ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1500),
                duty: Some(60.0),
                ..Default::default()
            },
            ChannelStatus {
                name: "power1".to_string(),
                watts: Some(40.0),
                ..Default::default()
            },
        ];
        let mut temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 48.0,
        }];
        let fresh_channels: HashSet<&str> = ["fan1", "power1"].into_iter().collect();
        let fresh_temps: HashSet<&str> = ["temp1"].into_iter().collect();

        fsd.overwrite_missing(&mut channels, &mut temps, &fresh_channels, &fresh_temps);

        // Cache values must be preserved exactly as they were.
        assert_eq!(channels.len(), 2);
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(1500));
        assert_eq!(fan1.duty, Some(60.0));
        let power1 = channels.iter().find(|c| c.name == "power1").unwrap();
        assert_eq!(power1.watts, Some(40.0));
        assert_eq!(temps.len(), 1);
        assert!((temps[0].temp - 48.0).abs() < f64::EPSILON);
    }

    #[test]
    fn overwrite_missing_replaces_only_absent_entry_in_place() {
        // When one expected name is absent from the fresh set, its
        // matching cache entry must be overwritten in place with the
        // failsafe value. All other cache entries must stay untouched.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te).unwrap();
        let mut channels = vec![
            ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1500),
                duty: Some(60.0),
                ..Default::default()
            },
            ChannelStatus {
                name: "power1".to_string(),
                watts: Some(40.0),
                ..Default::default()
            },
        ];
        let mut temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 48.0,
        }];
        // power1 did not report this tick; fan1 and temp1 did.
        let fresh_channels: HashSet<&str> = ["fan1"].into_iter().collect();
        let fresh_temps: HashSet<&str> = ["temp1"].into_iter().collect();

        fsd.overwrite_missing(&mut channels, &mut temps, &fresh_channels, &fresh_temps);

        // Length unchanged: overwrite in place, no push.
        assert_eq!(channels.len(), 2);
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(1500));
        assert_eq!(fan1.duty, Some(60.0));
        let power1 = channels.iter().find(|c| c.name == "power1").unwrap();
        assert_eq!(power1.watts, Some(MISSING_WATTS_FAILSAFE));
        assert_eq!(temps.len(), 1);
        assert!((temps[0].temp - 48.0).abs() < f64::EPSILON);
    }

    #[test]
    fn overwrite_missing_replaces_every_entry_when_all_absent() {
        // When no expected name appears in the fresh set, every cache
        // entry must be overwritten with its failsafe value.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te).unwrap();
        let mut channels = vec![
            ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1500),
                duty: Some(60.0),
                ..Default::default()
            },
            ChannelStatus {
                name: "power1".to_string(),
                watts: Some(40.0),
                ..Default::default()
            },
        ];
        let mut temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 48.0,
        }];
        let fresh_channels: HashSet<&str> = HashSet::new();
        let fresh_temps: HashSet<&str> = HashSet::new();

        fsd.overwrite_missing(&mut channels, &mut temps, &fresh_channels, &fresh_temps);

        assert_eq!(channels.len(), 2);
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(MISSING_RPM_FAILSAFE));
        assert_eq!(fan1.duty, Some(MISSING_DUTY_FAILSAFE));
        let power1 = channels.iter().find(|c| c.name == "power1").unwrap();
        assert_eq!(power1.watts, Some(MISSING_WATTS_FAILSAFE));
        assert_eq!(temps.len(), 1);
        assert!((temps[0].temp - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
    }

    #[test]
    fn overwrite_missing_ignores_unexpected_fresh_names() {
        // A fresh name for which no failsafe is defined must not cause
        // any change. The fresh set is only consulted to decide which
        // expected names are absent; unexpected names are irrelevant.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te).unwrap();
        let mut channels = vec![
            ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1500),
                duty: Some(60.0),
                ..Default::default()
            },
            ChannelStatus {
                name: "power1".to_string(),
                watts: Some(40.0),
                ..Default::default()
            },
        ];
        let mut temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 48.0,
        }];
        // "unknown_fan" has no failsafe; all expected names are present.
        let fresh_channels: HashSet<&str> = ["fan1", "power1", "unknown_fan"].into_iter().collect();
        let fresh_temps: HashSet<&str> = ["temp1"].into_iter().collect();

        fsd.overwrite_missing(&mut channels, &mut temps, &fresh_channels, &fresh_temps);

        // No failsafe substitution occurred; cache unchanged.
        assert_eq!(channels.len(), 2);
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(1500));
        let power1 = channels.iter().find(|c| c.name == "power1").unwrap();
        assert_eq!(power1.watts, Some(40.0));
        assert_eq!(temps.len(), 1);
        assert!((temps[0].temp - 48.0).abs() < f64::EPSILON);
    }

    // --- regression guard: full tick sequence with mixed reads ---

    /// Mirrors the upsert + `overwrite_missing` pattern used by
    /// `HwmonRepo::upsert_preloaded_statuses`. Pure function on the
    /// cache so the regression tests below can drive it directly.
    fn simulate_tick(
        fsd: &mut FailsafeStatusData,
        cached_channels: &mut Vec<ChannelStatus>,
        cached_temps: &mut Vec<TempStatus>,
        fresh_channels: Vec<ChannelStatus>,
        fresh_temps: Vec<TempStatus>,
        any_failure: bool,
    ) {
        let failsafe_active = if any_failure {
            fsd.record_failure()
        } else {
            fsd.record_success();
            false
        };
        let fresh_channel_names_owned: Vec<String> =
            fresh_channels.iter().map(|c| c.name.clone()).collect();
        let fresh_temp_names_owned: Vec<String> =
            fresh_temps.iter().map(|t| t.name.clone()).collect();
        for fresh in fresh_channels {
            if let Some(entry) = cached_channels.iter_mut().find(|c| c.name == fresh.name) {
                *entry = fresh;
            } else {
                cached_channels.push(fresh);
            }
        }
        for fresh in fresh_temps {
            if let Some(entry) = cached_temps.iter_mut().find(|t| t.name == fresh.name) {
                *entry = fresh;
            } else {
                cached_temps.push(fresh);
            }
        }
        if failsafe_active {
            let fresh_channel_names: HashSet<&str> = fresh_channel_names_owned
                .iter()
                .map(String::as_str)
                .collect();
            let fresh_temp_names: HashSet<&str> =
                fresh_temp_names_owned.iter().map(String::as_str).collect();
            fsd.overwrite_missing(
                cached_channels,
                cached_temps,
                &fresh_channel_names,
                &fresh_temp_names,
            );
        }
    }

    #[test]
    fn mixed_reads_serve_last_known_good_until_threshold_then_failsafe() {
        // Regression guard for the original user report. Temp A reads
        // successfully every tick with a real value; Temp B fails every
        // tick. Pre-threshold, B must keep serving its initial value
        // (48.0), never 0.0. Post-threshold, B must flip to the
        // failsafe (100.0). A must always serve its fresh value.
        let initial_temps = vec![
            TempStatus {
                name: "tempA".to_string(),
                temp: 40.0,
            },
            TempStatus {
                name: "tempB".to_string(),
                temp: 48.0,
            },
        ];
        let (ch, te) = create_failsafe_data(&[], &initial_temps);
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
        let mut cached_channels: Vec<ChannelStatus> = Vec::new();
        let mut cached_temps = initial_temps.clone();

        // Pre-threshold: MISSING_STATUS_THRESHOLD ticks of mixed reads.
        // Temp A keeps reporting fresh values; Temp B is always absent.
        for tick in 1..=MISSING_STATUS_THRESHOLD {
            #[allow(clippy::cast_precision_loss)]
            let fresh_a_value = 40.0 + tick as f64;
            let fresh_temps = vec![TempStatus {
                name: "tempA".to_string(),
                temp: fresh_a_value,
            }];
            simulate_tick(
                &mut fsd,
                &mut cached_channels,
                &mut cached_temps,
                Vec::new(),
                fresh_temps,
                true,
            );
            // Temp A tracks its fresh value.
            let temp_a = cached_temps.iter().find(|t| t.name == "tempA").unwrap();
            assert!((temp_a.temp - fresh_a_value).abs() < f64::EPSILON);
            // Temp B keeps its last-known-good reading (48.0).
            let temp_b = cached_temps.iter().find(|t| t.name == "tempB").unwrap();
            assert!(
                (temp_b.temp - 48.0).abs() < f64::EPSILON,
                "pre-threshold tick {tick}: tempB must be 48.0, got {}",
                temp_b.temp,
            );
            // Never 0.0 for the failing channel.
            assert!(temp_b.temp.abs() > f64::EPSILON);
        }

        // Post-threshold tick: Temp B flips to failsafe.
        let fresh_temps = vec![TempStatus {
            name: "tempA".to_string(),
            temp: 50.0,
        }];
        simulate_tick(
            &mut fsd,
            &mut cached_channels,
            &mut cached_temps,
            Vec::new(),
            fresh_temps,
            true,
        );
        let temp_a = cached_temps.iter().find(|t| t.name == "tempA").unwrap();
        assert!((temp_a.temp - 50.0).abs() < f64::EPSILON);
        let temp_b = cached_temps.iter().find(|t| t.name == "tempB").unwrap();
        assert!(
            (temp_b.temp - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON,
            "post-threshold: tempB must be MISSING_TEMP_FAILSAFE (100.0), got {}",
            temp_b.temp,
        );
    }

    #[test]
    fn recovery_after_failsafe_serves_fresh_values_only() {
        // After the failure counter trips the threshold and failsafe
        // values populate the cache, a full-success tick must reset
        // the counter and serve only real values. The failsafe overlay
        // must not apply when any_failure is false.
        let initial_temps = vec![TempStatus {
            name: "tempA".to_string(),
            temp: 40.0,
        }];
        let (ch, te) = create_failsafe_data(&[], &initial_temps);
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
        let mut cached_channels: Vec<ChannelStatus> = Vec::new();
        let mut cached_temps = initial_temps.clone();

        // Drive the counter past the threshold with all-failure ticks.
        for _ in 0..=MISSING_STATUS_THRESHOLD {
            simulate_tick(
                &mut fsd,
                &mut cached_channels,
                &mut cached_temps,
                Vec::new(),
                Vec::new(),
                true,
            );
        }
        assert!(fsd.threshold_exceeded());
        let temp_a = cached_temps.iter().find(|t| t.name == "tempA").unwrap();
        assert!((temp_a.temp - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);

        // Full success: counter resets, cache serves fresh value only.
        let fresh_temps = vec![TempStatus {
            name: "tempA".to_string(),
            temp: 55.0,
        }];
        simulate_tick(
            &mut fsd,
            &mut cached_channels,
            &mut cached_temps,
            Vec::new(),
            fresh_temps,
            false,
        );
        assert!(fsd.threshold_exceeded().not());
        let temp_a = cached_temps.iter().find(|t| t.name == "tempA").unwrap();
        assert!((temp_a.temp - 55.0).abs() < f64::EPSILON);
    }

    #[test]
    fn build_failsafe_status_has_correct_values() {
        // The built status must contain the actual failsafe constant
        // values, not the original device readings.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let fsd = FailsafeStatusData::new(ch, te).unwrap();
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
}
