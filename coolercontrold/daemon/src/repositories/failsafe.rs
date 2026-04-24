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
#[cfg(test)]
use std::collections::HashSet;
use std::ops::Not;

use crate::device::{ChannelName, ChannelStatus, Mhz, Status, Temp, TempStatus, Watts, RPM};

/// Consecutive missing status readings before failsafe values activate.
/// Also the multiplier applied to `poll_rate` by the per-device wait
/// timeouts in hwmon, gpu, and service-plugin repositories, so a write
/// held to its maximum and a read path declaring the device lost
/// coincide by construction rather than race.
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

/// Per-channel state tracked by hwmon's per-channel staleness path.
/// One instance per known channel / temp name, seeded at device
/// init. All three fields are pre-allocated so the hot path never
/// allocates or clones the channel name.
#[derive(Default)]
pub struct FailsafeTickState {
    /// Consecutive ticks where this channel did not report fresh.
    /// Saturates at `u16::MAX` to avoid wrap.
    pub stale_ticks: u16,
    /// Set to true by a streaming sink the moment it upserts this
    /// channel in the current preload attempt, and cleared by
    /// `reset_fresh_this_tick` at the start of the next attempt.
    /// Read by `tick_per_channel_staleness` to decide whether to
    /// reset the counter or tick it up.
    pub fresh_this_tick: bool,
    /// True while this channel's cache entry holds its failsafe
    /// value. Lets `tick_per_channel_staleness` clone the failsafe
    /// value into the cache exactly once on the transition tick,
    /// not on every subsequent stale tick.
    pub is_failsafed: bool,
}

/// Tracks consecutive missing sensor readings for a single device and
/// holds pre-built failsafe channel/temp values to substitute when the
/// threshold is exceeded.
pub struct FailsafeStatusData {
    pub count: usize,
    pub logged: bool,
    pub channel_failsafes: HashMap<ChannelName, ChannelStatus>,
    pub temp_failsafes: HashMap<ChannelName, TempStatus>,
    /// Per-channel state for hwmon's per-channel staleness path.
    /// Each device channel / temp has its own counter and
    /// fresh / failsafed flags so only the actually-stale ones are
    /// substituted with failsafe values on timeout. Unused by
    /// `liquidctl` / `service_plugin`, which continue to use the
    /// device-level `count` via `record_failure` / `record_success`.
    pub channel_state: HashMap<ChannelName, FailsafeTickState>,
    pub temp_state: HashMap<ChannelName, FailsafeTickState>,
    /// Whether any per-channel counter was above threshold on the
    /// previous `tick_per_channel_staleness` call. Used to emit
    /// one-shot transition logs ("Significant issue..." on first
    /// crossing, "Recovered..." on full recovery).
    pub was_failsafing: bool,
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
        // Pre-size both state maps and seed one default entry per
        // known name. The hot path only ever mutates existing
        // entries via `get_mut`, so no further allocation.
        let mut channel_state = HashMap::with_capacity(channel_failsafes.len());
        for name in channel_failsafes.keys() {
            channel_state.insert(name.clone(), FailsafeTickState::default());
        }
        let mut temp_state = HashMap::with_capacity(temp_failsafes.len());
        for name in temp_failsafes.keys() {
            temp_state.insert(name.clone(), FailsafeTickState::default());
        }
        debug_assert_eq!(channel_state.len(), channel_failsafes.len());
        debug_assert_eq!(temp_state.len(), temp_failsafes.len());
        Some(Self {
            count: 0,
            logged: false,
            channel_failsafes,
            temp_failsafes,
            channel_state,
            temp_state,
            was_failsafing: false,
        })
    }

    /// Marks a channel as freshly upserted in the current preload
    /// attempt. No-op when `name` is not a known channel, so a
    /// streaming sink carrying a renamed or phantom channel cannot
    /// introduce new keys or allocate.
    pub fn mark_channel_fresh(&mut self, name: &str) {
        if let Some(state) = self.channel_state.get_mut(name) {
            state.fresh_this_tick = true;
        }
    }

    /// Mirror of `mark_channel_fresh` for temps.
    pub fn mark_temp_fresh(&mut self, name: &str) {
        if let Some(state) = self.temp_state.get_mut(name) {
            state.fresh_this_tick = true;
        }
    }

    /// Clears every `fresh_this_tick` flag. Called at the start of
    /// each new hwmon preload attempt so the flags reflect only the
    /// in-flight attempt. `is_failsafed` and `stale_ticks` persist
    /// across preloads.
    pub fn reset_fresh_this_tick(&mut self) {
        for state in self.channel_state.values_mut() {
            state.fresh_this_tick = false;
        }
        for state in self.temp_state.values_mut() {
            state.fresh_this_tick = false;
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
    /// Returns true when the device has just recovered from an
    /// above-threshold state, so callers can emit a one-shot recovery
    /// log. Also clears the `logged` flag so a subsequent threshold
    /// breach re-logs its error.
    pub fn record_success(&mut self) -> bool {
        let recovered = self.count > MISSING_STATUS_THRESHOLD;
        if self.count > 0 {
            self.count = 0;
        }
        if recovered {
            self.logged = false;
        }
        recovered
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
    #[cfg(test)]
    fn overwrite_missing(
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

    /// Advances per-channel stale-tick counters from the
    /// `fresh_this_tick` flags the streaming sinks set during the
    /// current preload, and substitutes failsafe values for any
    /// channel whose counter has crossed `MISSING_STATUS_THRESHOLD`.
    ///
    /// The failsafe value is cloned into the cache exactly once, on
    /// the tick the channel crosses into failsafed state, not on
    /// every subsequent stale tick. A streaming sink that later
    /// upserts a real reading flips `is_failsafed` back to false via
    /// the `fresh_this_tick` reset branch.
    ///
    /// Bounded at `u16::MAX` via `saturating_add`. Returns
    /// `(newly_failsafing, just_recovered)` transition booleans so
    /// the caller can emit one-shot log lines. Used by hwmon, where
    /// a device's sensors can go stale independently; `liquidctl`
    /// and `service_plugin` stay on the device-level
    /// `record_failure` / `record_success` path.
    pub fn tick_per_channel_staleness(
        &mut self,
        channels: &mut Vec<ChannelStatus>,
        temps: &mut Vec<TempStatus>,
    ) -> (bool, bool) {
        let channels_before = channels.len();
        let temps_before = temps.len();
        let mut any_failsafing = false;
        // Disjoint field borrows: `channel_state` is iterated
        // mutably while `channel_failsafes` is read immutably. Rust
        // accepts this via split-borrow on self.
        let channel_failsafes = &self.channel_failsafes;
        for (name, state) in &mut self.channel_state {
            Self::advance_channel_tick(name, state, channel_failsafes, channels);
            if state.is_failsafed {
                any_failsafing = true;
            }
        }
        let temp_failsafes = &self.temp_failsafes;
        for (name, state) in &mut self.temp_state {
            Self::advance_temp_tick(name, state, temp_failsafes, temps);
            if state.is_failsafed {
                any_failsafing = true;
            }
        }
        let newly_failsafing = any_failsafing && self.was_failsafing.not();
        let just_recovered = any_failsafing.not() && self.was_failsafing;
        self.was_failsafing = any_failsafing;
        // Monotonicity: entries are replaced in place or appended;
        // never removed.
        debug_assert!(channels.len() >= channels_before);
        debug_assert!(temps.len() >= temps_before);
        (newly_failsafing, just_recovered)
    }

    /// Advances a single channel's stale counter and, on the
    /// cross-threshold transition, writes the failsafe value into
    /// the cache exactly once. Pure leaf helper; no `self`.
    fn advance_channel_tick(
        name: &ChannelName,
        state: &mut FailsafeTickState,
        channel_failsafes: &HashMap<ChannelName, ChannelStatus>,
        channels: &mut Vec<ChannelStatus>,
    ) {
        if state.fresh_this_tick {
            state.stale_ticks = 0;
            state.is_failsafed = false;
            debug_assert!(state.is_failsafed.not());
            return;
        }
        state.stale_ticks = state.stale_ticks.saturating_add(1);
        debug_assert!(state.stale_ticks > 0);
        if (state.stale_ticks as usize) <= MISSING_STATUS_THRESHOLD {
            return;
        }
        if state.is_failsafed {
            return;
        }
        if let Some(failsafe) = channel_failsafes.get(name) {
            if let Some(entry) = channels.iter_mut().find(|c| &c.name == name) {
                *entry = failsafe.clone();
            } else {
                channels.push(failsafe.clone());
            }
            state.is_failsafed = true;
        }
    }

    /// Mirror of `advance_channel_tick` for temps.
    fn advance_temp_tick(
        name: &ChannelName,
        state: &mut FailsafeTickState,
        temp_failsafes: &HashMap<ChannelName, TempStatus>,
        temps: &mut Vec<TempStatus>,
    ) {
        if state.fresh_this_tick {
            state.stale_ticks = 0;
            state.is_failsafed = false;
            debug_assert!(state.is_failsafed.not());
            return;
        }
        state.stale_ticks = state.stale_ticks.saturating_add(1);
        debug_assert!(state.stale_ticks > 0);
        if (state.stale_ticks as usize) <= MISSING_STATUS_THRESHOLD {
            return;
        }
        if state.is_failsafed {
            return;
        }
        if let Some(failsafe) = temp_failsafes.get(name) {
            if let Some(entry) = temps.iter_mut().find(|t| &t.name == name) {
                *entry = failsafe.clone();
            } else {
                temps.push(failsafe.clone());
            }
            state.is_failsafed = true;
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
        let recovered = fsd.record_success();
        assert_eq!(fsd.count, 0);
        assert!(fsd.threshold_exceeded().not());
        // Below threshold: success is not a "recovery" event.
        assert!(recovered.not());
    }

    #[test]
    fn record_success_noop_at_zero() {
        // Resetting at zero must not underflow or cause issues.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
        let recovered = fsd.record_success();
        assert_eq!(fsd.count, 0);
        assert!(recovered.not());
    }

    #[test]
    fn record_success_signals_recovery_from_above_threshold() {
        // Transitioning from above MISSING_STATUS_THRESHOLD back to
        // zero is the one case where record_success must return true
        // so the caller can emit a recovery log. The logged flag must
        // also clear so a subsequent breach re-logs its error.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
        for _ in 0..=MISSING_STATUS_THRESHOLD {
            fsd.record_failure();
        }
        assert!(fsd.threshold_exceeded());
        assert!(fsd.log_once());
        assert!(fsd.logged);
        let recovered = fsd.record_success();
        assert!(recovered);
        assert_eq!(fsd.count, 0);
        assert!(fsd.logged.not());
    }

    #[test]
    fn record_success_no_recovery_at_exact_threshold() {
        // At exactly MISSING_STATUS_THRESHOLD failures the device has
        // not tripped failsafe, so a subsequent success is just a
        // reset, not a recovery. Returning true here would produce a
        // spurious "recovered" log.
        let (ch, te) = create_failsafe_data(&sample_channels(), &sample_temps());
        let mut fsd = FailsafeStatusData::new(ch, te).unwrap();
        for _ in 0..MISSING_STATUS_THRESHOLD {
            fsd.record_failure();
        }
        assert_eq!(fsd.count, MISSING_STATUS_THRESHOLD);
        assert!(fsd.threshold_exceeded().not());
        let recovered = fsd.record_success();
        assert!(recovered.not());
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

    // --- tick_per_channel_staleness: per-channel hwmon path ---

    fn make_fsd_for_staleness_tests() -> FailsafeStatusData {
        // Three expected names: fan1, fan2 (channels) and temp1.
        let channels = vec![
            ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                freq: None,
                watts: None,
                pwm_mode: None,
            },
            ChannelStatus {
                name: "fan2".to_string(),
                rpm: Some(900),
                duty: Some(30.0),
                freq: None,
                watts: None,
                pwm_mode: None,
            },
        ];
        let temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 40.0,
        }];
        let (ch, te) = create_failsafe_data(&channels, &temps);
        FailsafeStatusData::new(ch, te).unwrap()
    }

    fn starting_cache() -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        (
            vec![
                ChannelStatus {
                    name: "fan1".to_string(),
                    rpm: Some(1200),
                    duty: Some(50.0),
                    freq: None,
                    watts: None,
                    pwm_mode: None,
                },
                ChannelStatus {
                    name: "fan2".to_string(),
                    rpm: Some(900),
                    duty: Some(30.0),
                    freq: None,
                    watts: None,
                    pwm_mode: None,
                },
            ],
            vec![TempStatus {
                name: "temp1".to_string(),
                temp: 40.0,
            }],
        )
    }

    /// Marks every known channel and temp fresh for one tick, then
    /// advances the staleness state via `tick_per_channel_staleness`.
    fn tick_with_all_fresh(
        fsd: &mut FailsafeStatusData,
        channels: &mut Vec<ChannelStatus>,
        temps: &mut Vec<TempStatus>,
    ) -> (bool, bool) {
        fsd.reset_fresh_this_tick();
        fsd.mark_channel_fresh("fan1");
        fsd.mark_channel_fresh("fan2");
        fsd.mark_temp_fresh("temp1");
        fsd.tick_per_channel_staleness(channels, temps)
    }

    /// Marks only the given channel fresh for one tick (no temps),
    /// then advances staleness state.
    fn tick_with_one_fresh_channel(
        fsd: &mut FailsafeStatusData,
        channels: &mut Vec<ChannelStatus>,
        temps: &mut Vec<TempStatus>,
        name: &str,
    ) -> (bool, bool) {
        fsd.reset_fresh_this_tick();
        fsd.mark_channel_fresh(name);
        fsd.tick_per_channel_staleness(channels, temps)
    }

    #[test]
    fn tick_per_channel_never_failsafes_a_consistently_fresh_channel() {
        // Every known name is fresh on every tick. Counters must stay 0,
        // cache must keep its real values, no failsafing transition.
        let mut fsd = make_fsd_for_staleness_tests();
        let (mut channels, mut temps) = starting_cache();
        for _ in 0..(MISSING_STATUS_THRESHOLD * 2) {
            let (newly, recovered) = tick_with_all_fresh(&mut fsd, &mut channels, &mut temps);
            assert!(newly.not());
            assert!(recovered.not());
        }
        assert_eq!(fsd.channel_state["fan1"].stale_ticks, 0);
        assert_eq!(fsd.channel_state["fan2"].stale_ticks, 0);
        assert_eq!(fsd.temp_state["temp1"].stale_ticks, 0);
        assert!(fsd.channel_state["fan1"].is_failsafed.not());
        assert!(fsd.was_failsafing.not());
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(1200));
        let temp_entry = temps.iter().find(|t| t.name == "temp1").unwrap();
        assert!((temp_entry.temp - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tick_per_channel_failsafes_only_the_stale_channel() {
        // Only fan1 reports fresh each tick. fan2 and temp1 must tick up
        // and cross the threshold, flipping to their failsafes, while
        // fan1 stays untouched. newly_failsafing fires exactly once, at
        // the threshold crossing.
        let mut fsd = make_fsd_for_staleness_tests();
        let (mut channels, mut temps) = starting_cache();
        let mut newly_count = 0_u32;
        for _ in 0..=MISSING_STATUS_THRESHOLD {
            let (newly, recovered) =
                tick_with_one_fresh_channel(&mut fsd, &mut channels, &mut temps, "fan1");
            if newly {
                newly_count += 1;
            }
            assert!(recovered.not());
        }
        assert_eq!(newly_count, 1);
        assert!(fsd.was_failsafing);
        assert_eq!(fsd.channel_state["fan1"].stale_ticks, 0);
        assert!(fsd.channel_state["fan1"].is_failsafed.not());
        assert!(fsd.channel_state["fan2"].stale_ticks as usize > MISSING_STATUS_THRESHOLD);
        assert!(fsd.channel_state["fan2"].is_failsafed);
        assert!(fsd.temp_state["temp1"].stale_ticks as usize > MISSING_STATUS_THRESHOLD);
        assert!(fsd.temp_state["temp1"].is_failsafed);
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(1200));
        let fan2 = channels.iter().find(|c| c.name == "fan2").unwrap();
        assert_eq!(fan2.rpm, Some(MISSING_RPM_FAILSAFE));
        assert_eq!(fan2.duty, Some(MISSING_DUTY_FAILSAFE));
        let temp_entry = temps.iter().find(|t| t.name == "temp1").unwrap();
        assert!((temp_entry.temp - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
    }

    #[test]
    fn tick_per_channel_clones_failsafe_only_on_transition() {
        // The failsafe value is written into the cache exactly once,
        // on the tick the counter crosses the threshold. Subsequent
        // stale ticks leave the cache alone even though the channel
        // stays failsafed. Verified by mutating the cache entry
        // between ticks and checking that the mutation persists.
        let mut fsd = make_fsd_for_staleness_tests();
        let (mut channels, mut temps) = starting_cache();
        for _ in 0..=MISSING_STATUS_THRESHOLD {
            tick_with_one_fresh_channel(&mut fsd, &mut channels, &mut temps, "fan1");
        }
        assert!(fsd.channel_state["fan2"].is_failsafed);
        let fan2 = channels.iter().find(|c| c.name == "fan2").unwrap();
        assert_eq!(fan2.rpm, Some(MISSING_RPM_FAILSAFE));
        // Mutate fan2 in place to a sentinel; if tick_per_channel
        // were re-cloning the failsafe every tick, the sentinel
        // would get overwritten.
        let sentinel_rpm = Some(4242);
        let fan2_mut = channels.iter_mut().find(|c| c.name == "fan2").unwrap();
        fan2_mut.rpm = sentinel_rpm;
        // Tick once more with fan2 still absent from fresh.
        tick_with_one_fresh_channel(&mut fsd, &mut channels, &mut temps, "fan1");
        let fan2 = channels.iter().find(|c| c.name == "fan2").unwrap();
        assert_eq!(fan2.rpm, sentinel_rpm);
    }

    #[test]
    fn tick_per_channel_recovers_when_channel_returns_fresh() {
        // After driving fan2 and temp1 into failsafe, marking every
        // channel fresh (with new real values in the cache) must
        // reset counters to 0, clear is_failsafed, keep the fresh
        // cache values, and fire just_recovered exactly once.
        let mut fsd = make_fsd_for_staleness_tests();
        let (mut channels, mut temps) = starting_cache();
        for _ in 0..=MISSING_STATUS_THRESHOLD {
            tick_with_one_fresh_channel(&mut fsd, &mut channels, &mut temps, "fan1");
        }
        assert!(fsd.was_failsafing);
        // Simulate every channel fresh with new real values in the
        // cache (as a real sink would have upserted).
        for channel in &mut channels {
            match channel.name.as_str() {
                "fan1" => channel.rpm = Some(1300),
                "fan2" => channel.rpm = Some(950),
                _ => {}
            }
        }
        for temp in &mut temps {
            if temp.name == "temp1" {
                temp.temp = 42.5;
            }
        }
        let (newly, recovered) = tick_with_all_fresh(&mut fsd, &mut channels, &mut temps);
        assert!(newly.not());
        assert!(recovered);
        assert_eq!(fsd.channel_state["fan2"].stale_ticks, 0);
        assert!(fsd.channel_state["fan2"].is_failsafed.not());
        assert_eq!(fsd.temp_state["temp1"].stale_ticks, 0);
        assert!(fsd.temp_state["temp1"].is_failsafed.not());
        assert!(fsd.was_failsafing.not());
        let fan2 = channels.iter().find(|c| c.name == "fan2").unwrap();
        assert_eq!(fan2.rpm, Some(950));
        let temp_entry = temps.iter().find(|t| t.name == "temp1").unwrap();
        assert!((temp_entry.temp - 42.5).abs() < f64::EPSILON);
    }

    #[test]
    fn tick_per_channel_counter_saturates_at_u16_max() {
        // Poke the counter to u16::MAX - 1 and tick stale; it must
        // reach u16::MAX and stop incrementing on the next stale tick.
        let mut fsd = make_fsd_for_staleness_tests();
        let (mut channels, mut temps) = starting_cache();
        fsd.channel_state.get_mut("fan2").unwrap().stale_ticks = u16::MAX - 1;
        fsd.reset_fresh_this_tick();
        fsd.mark_temp_fresh("temp1");
        fsd.tick_per_channel_staleness(&mut channels, &mut temps);
        assert_eq!(fsd.channel_state["fan2"].stale_ticks, u16::MAX);
        fsd.reset_fresh_this_tick();
        fsd.mark_temp_fresh("temp1");
        fsd.tick_per_channel_staleness(&mut channels, &mut temps);
        assert_eq!(fsd.channel_state["fan2"].stale_ticks, u16::MAX);
    }

    #[test]
    fn mark_fresh_ignores_unexpected_names() {
        // A fresh-name not in the expected set must be a no-op: it
        // must not panic and must not allocate a new state entry.
        let mut fsd = make_fsd_for_staleness_tests();
        let (mut channels, mut temps) = starting_cache();
        let initial_len = fsd.channel_state.len();
        fsd.reset_fresh_this_tick();
        fsd.mark_channel_fresh("fan1");
        fsd.mark_channel_fresh("ghost_channel");
        fsd.mark_temp_fresh("temp1");
        fsd.mark_temp_fresh("ghost_temp");
        fsd.tick_per_channel_staleness(&mut channels, &mut temps);
        assert_eq!(fsd.channel_state.len(), initial_len);
        assert_eq!(fsd.channel_state["fan1"].stale_ticks, 0);
        assert_eq!(fsd.channel_state["fan2"].stale_ticks, 1);
        assert_eq!(fsd.temp_state["temp1"].stale_ticks, 0);
        assert!(fsd.channel_state.contains_key("ghost_channel").not());
        assert!(fsd.temp_state.contains_key("ghost_temp").not());
    }
}
