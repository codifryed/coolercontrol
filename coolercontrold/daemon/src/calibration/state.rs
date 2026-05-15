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

//! Per-channel runtime state for the calibrated dispatch layer.
//!
//! The state is intentionally small. Transition logic lives in
//! `dispatch.rs`; this file only owns the data structures and the
//! protected read/write primitives.

use super::ChannelKey;
use crate::device::Duty;
use std::cell::RefCell;
use std::collections::HashMap;

/// Snapshot of a channel's relationship to the calibrated dispatcher.
///
/// Carrying both the fan state and the `under_diagnosis` flag together
/// lets a single map lookup decide all the dispatch branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelEntry {
    pub state: FanState,
    pub under_diagnosis: bool,
}

impl ChannelEntry {
    /// The default for any channel never observed by the dispatcher:
    /// idle, not under diagnosis.
    pub fn default_off() -> Self {
        Self {
            state: FanState::Off,
            under_diagnosis: false,
        }
    }
}

/// What the dispatcher believes the fan is doing right now.
///
/// `Kicking` carries the target sustain duty the deferred task will
/// write when the kick window elapses. The dispatcher updates this in
/// place when subsequent ticks pick a new target while the fan is still
/// kicking, so re-kicking is avoided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanState {
    Off,
    Kicking { sustain_target: Duty },
    On,
}

#[allow(dead_code)] // predicate helpers; dispatch uses pattern matching directly.
impl FanState {
    pub fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    pub fn is_kicking(self) -> bool {
        matches!(self, Self::Kicking { .. })
    }

    pub fn is_on(self) -> bool {
        matches!(self, Self::On)
    }

    pub fn sustain_target(self) -> Option<Duty> {
        match self {
            Self::Kicking { sustain_target } => Some(sustain_target),
            Self::Off | Self::On => None,
        }
    }
}

/// Map of channel state, protected by a `RefCell` for single-threaded
/// async access. The single-threaded runtime guarantees no concurrent
/// borrow can occur as long as we keep mutations between yield points.
pub struct FanStateMap {
    inner: RefCell<HashMap<ChannelKey, ChannelEntry>>,
}

impl Default for FanStateMap {
    fn default() -> Self {
        Self::new()
    }
}

impl FanStateMap {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(HashMap::new()),
        }
    }

    /// Read a channel's entry. Returns the default (`Off`, not under
    /// diagnosis) for channels never touched before.
    pub fn entry(&self, key: &ChannelKey) -> ChannelEntry {
        self.inner
            .borrow()
            .get(key)
            .copied()
            .unwrap_or_else(ChannelEntry::default_off)
    }

    /// Write a channel's entry. Always replaces; the dispatcher decides
    /// the new value before calling.
    pub fn replace(&self, key: ChannelKey, entry: ChannelEntry) {
        self.inner.borrow_mut().insert(key, entry);
    }

    /// Whether a channel is currently under a diagnosis sweep.
    pub fn is_under_diagnosis(&self, key: &ChannelKey) -> bool {
        self.inner
            .borrow()
            .get(key)
            .is_some_and(|entry| entry.under_diagnosis)
    }

    /// Toggle a channel's `under_diagnosis` flag, preserving its state.
    /// The diagnoser calls this with `false` at completion (success or
    /// failure alike); for `true`, it should use `begin_diagnosis`
    /// which also forces the `FanState` to `Off`.
    pub fn set_under_diagnosis(&self, key: ChannelKey, value: bool) {
        let mut inner = self.inner.borrow_mut();
        let entry = inner.entry(key).or_insert_with(ChannelEntry::default_off);
        entry.under_diagnosis = value;
    }

    /// Mark a channel `under_diagnosis` AND reset its `FanState` to `Off`.
    /// Resetting to `Off` prevents a stale pre-diagnosis `Kicking` from
    /// firing its deferred sustain-write over the sweep's raw writes,
    /// and leaves the post-sweep restore in a clean state to do a fresh
    /// `Off -> Kicking` under the new calibration.
    pub fn begin_diagnosis(&self, key: ChannelKey) {
        self.inner.borrow_mut().insert(
            key,
            ChannelEntry {
                state: FanState::Off,
                under_diagnosis: true,
            },
        );
    }

    /// Drop the entry. Used when calibration is cleared.
    pub fn forget(&self, key: &ChannelKey) {
        self.inner.borrow_mut().remove(key);
    }

    #[allow(dead_code)] // test-only; useful production API.
    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    #[allow(dead_code)] // test-only; useful production API.
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    fn k(dev: &str, chan: &str) -> ChannelKey {
        (dev.to_string(), chan.to_string())
    }

    #[test]
    fn fresh_map_is_empty() {
        // Goal: a brand new map reports zero entries and returns the
        // Off default for any key the dispatcher asks about. This is
        // the entry path for any channel on first observation.
        let map = FanStateMap::new();
        assert_eq!(map.len(), 0);
        assert!(map.is_empty());
        let entry = map.entry(&k("dev-a", "fan1"));
        assert_eq!(entry.state, FanState::Off);
        assert!(entry.under_diagnosis.not());
    }

    #[test]
    fn replace_then_entry_returns_value() {
        // Goal: replace stores the value verbatim; subsequent entry()
        // returns the same observed state. The dispatcher relies on
        // round-trip identity to maintain its state machine.
        let map = FanStateMap::new();
        let key = k("dev-a", "fan1");
        let target = ChannelEntry {
            state: FanState::Kicking { sustain_target: 30 },
            under_diagnosis: false,
        };
        map.replace(key.clone(), target);
        assert_eq!(map.entry(&key), target);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn replace_overwrites_previous_state() {
        // Goal: a second replace on the same key must drop the prior
        // value, not append. The state machine writes each transition
        // as a full ChannelEntry replacement.
        let map = FanStateMap::new();
        let key = k("dev-a", "fan1");
        map.replace(
            key.clone(),
            ChannelEntry {
                state: FanState::Kicking { sustain_target: 30 },
                under_diagnosis: false,
            },
        );
        map.replace(
            key.clone(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
            },
        );
        assert_eq!(map.entry(&key).state, FanState::On);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn set_under_diagnosis_round_trips() {
        // Goal: setting under_diagnosis to true is observable via the
        // is_under_diagnosis getter; setting back to false is too.
        let map = FanStateMap::new();
        let key = k("dev-a", "fan1");
        assert!(map.is_under_diagnosis(&key).not());
        map.set_under_diagnosis(key.clone(), true);
        assert!(map.is_under_diagnosis(&key));
        map.set_under_diagnosis(key.clone(), false);
        assert!(map.is_under_diagnosis(&key).not());
    }

    #[test]
    fn set_under_diagnosis_preserves_fan_state() {
        // Goal: toggling the diagnosis flag must not stomp the fan
        // state field. The dispatcher needs to remember the fan's
        // last-known state for after the diagnosis completes.
        let map = FanStateMap::new();
        let key = k("dev-a", "fan1");
        map.replace(
            key.clone(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
            },
        );
        map.set_under_diagnosis(key.clone(), true);
        let observed = map.entry(&key);
        assert_eq!(observed.state, FanState::On);
        assert!(observed.under_diagnosis);
    }

    #[test]
    fn forget_drops_entry() {
        // Goal: forget() removes the entry entirely so the next
        // entry() lookup yields the Off default. Used when a channel's
        // calibration is cleared by the user.
        let map = FanStateMap::new();
        let key = k("dev-a", "fan1");
        map.replace(
            key.clone(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
            },
        );
        assert_eq!(map.len(), 1);
        map.forget(&key);
        assert_eq!(map.len(), 0);
        assert_eq!(map.entry(&key).state, FanState::Off);
    }

    #[test]
    fn begin_diagnosis_resets_kicking_state_to_off() {
        // Goal: when a sweep starts on a channel that was mid-kick, the
        // dispatcher must reset FanState to Off so the pending complete_kick
        // task observes non-Kicking and skips its write. Otherwise the
        // stale sustain duty lands on hardware mid-sweep.
        let map = FanStateMap::new();
        let key = k("dev-a", "fan1");
        map.replace(
            key.clone(),
            ChannelEntry {
                state: FanState::Kicking { sustain_target: 60 },
                under_diagnosis: false,
            },
        );
        map.begin_diagnosis(key.clone());
        let observed = map.entry(&key);
        assert_eq!(observed.state, FanState::Off);
        assert!(observed.under_diagnosis);
    }

    #[test]
    fn begin_diagnosis_resets_on_state_to_off() {
        // Goal: a channel running steady (FanState=On) must also be
        // reset to Off at sweep start so the post-sweep restore (which
        // dispatches a non-zero duty) goes through Off -> Kicking and
        // properly re-spins the fan from the 0% it ended the sweep at.
        let map = FanStateMap::new();
        let key = k("dev-a", "fan1");
        map.replace(
            key.clone(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
            },
        );
        map.begin_diagnosis(key.clone());
        let observed = map.entry(&key);
        assert_eq!(observed.state, FanState::Off);
        assert!(observed.under_diagnosis);
    }

    #[test]
    fn fan_state_predicates_are_consistent() {
        // Goal: exactly one of is_off / is_kicking / is_on returns
        // true for any FanState value. This is what the dispatcher
        // pattern-matches on for transitions.
        let off = FanState::Off;
        let kicking = FanState::Kicking { sustain_target: 20 };
        let on = FanState::On;
        assert!(off.is_off() && off.is_kicking().not() && off.is_on().not());
        assert!(kicking.is_off().not() && kicking.is_kicking() && kicking.is_on().not());
        assert!(on.is_off().not() && on.is_kicking().not() && on.is_on());
        assert_eq!(kicking.sustain_target(), Some(20));
        assert_eq!(off.sustain_target(), None);
        assert_eq!(on.sustain_target(), None);
    }
}
