// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-channel runtime state for the calibrated dispatch layer.
//!
//! The state is intentionally small. Transition logic lives in
//! `dispatch.rs`; this file only owns the data structures and the
//! protected read/write primitives.

use super::{ChannelKey, ChannelRef};
use crate::device::Duty;
use indexmap::{Equivalent, IndexMap};
use std::cell::RefCell;
use std::hash::Hash;

/// Snapshot of a channel's relationship to the calibrated dispatcher.
///
/// Carries the fan state, the `under_diagnosis` flag, and the last
/// true-duty the dispatcher commanded for this channel. The commanded
/// value lets the status augmenter recover the user-facing duty exactly
/// when the hardware-reported device duty's preimage still contains it,
/// closing the residual cross-fan drift the stateless reverse map can
/// not eliminate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelEntry {
    pub state: FanState,
    pub under_diagnosis: bool,
    pub commanded_true_duty: Option<Duty>,
}

impl ChannelEntry {
    /// The default for any channel never observed by the dispatcher:
    /// idle, not under diagnosis, no commanded value yet.
    pub fn default_off() -> Self {
        Self {
            state: FanState::Off,
            under_diagnosis: false,
            commanded_true_duty: None,
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

/// Looked up by `&ChannelKey` or by `&ChannelRef`, so the duty write path builds no owned key.
pub struct FanStateMap {
    inner: RefCell<IndexMap<ChannelKey, ChannelEntry>>,
}

impl Default for FanStateMap {
    fn default() -> Self {
        Self::new()
    }
}

impl FanStateMap {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(IndexMap::new()),
        }
    }

    /// Read a channel's entry. Returns the default (`Off`, not under
    /// diagnosis) for channels never touched before.
    pub fn entry<Q>(&self, key: &Q) -> ChannelEntry
    where
        Q: Hash + Equivalent<ChannelKey> + ?Sized,
    {
        self.inner
            .borrow()
            .get(key)
            .copied()
            .unwrap_or_else(ChannelEntry::default_off)
    }

    /// Write a channel's entry. Always replaces; the dispatcher decides
    /// the new value before calling. Only a channel's first entry builds an
    /// owned key.
    pub fn replace(&self, key: ChannelRef<'_>, entry: ChannelEntry) {
        let mut inner = self.inner.borrow_mut();
        if let Some(stored) = inner.get_mut(&key) {
            *stored = entry;
        } else {
            inner.insert(key.to_key(), entry);
        }
        debug_assert_eq!(inner.get(&key), Some(&entry));
    }

    /// Whether a channel is currently under a diagnosis sweep.
    pub fn is_under_diagnosis<Q>(&self, key: &Q) -> bool
    where
        Q: Hash + Equivalent<ChannelKey> + ?Sized,
    {
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
    /// `Off -> Kicking` under the new calibration. Clears the commanded
    /// cache so the post-sweep augmenter does not match stale values
    /// against the new calibration's curves.
    pub fn begin_diagnosis(&self, key: ChannelKey) {
        self.inner.borrow_mut().insert(
            key,
            ChannelEntry {
                state: FanState::Off,
                under_diagnosis: true,
                commanded_true_duty: None,
            },
        );
    }

    /// Last true-duty the dispatcher commanded for this channel.
    /// `None` for channels never written by us (uncalibrated pass-through,
    /// post-diagnosis, fresh startup) or written externally.
    pub fn commanded_true_duty<Q>(&self, key: &Q) -> Option<Duty>
    where
        Q: Hash + Equivalent<ChannelKey> + ?Sized,
    {
        self.inner
            .borrow()
            .get(key)
            .and_then(|entry| entry.commanded_true_duty)
    }

    /// Drop the entry. Used when calibration is cleared.
    pub fn forget(&self, key: &ChannelKey) {
        self.inner.borrow_mut().swap_remove(key);
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
            commanded_true_duty: None,
        };
        map.replace((&key).into(), target);
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
            (&key).into(),
            ChannelEntry {
                state: FanState::Kicking { sustain_target: 30 },
                under_diagnosis: false,
                commanded_true_duty: None,
            },
        );
        map.replace(
            (&key).into(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
                commanded_true_duty: None,
            },
        );
        assert_eq!(map.entry(&key).state, FanState::On);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn a_borrowed_key_reaches_the_entry_an_owned_key_stored() {
        // Goal: `ChannelRef` hashes and compares as the `ChannelKey` it names, so either form
        // reaches the same entry. Method: write under one form and read under the other, both
        // ways, then look up a sibling channel that was never stored.
        let map = FanStateMap::new();
        let key = k("dev-a", "fan1");
        let on = ChannelEntry {
            state: FanState::On,
            under_diagnosis: false,
            commanded_true_duty: Some(40),
        };

        map.replace(ChannelRef::new(&key.0, "fan1"), on);
        map.set_under_diagnosis(key.clone(), true);

        assert_eq!(map.entry(&key).state, FanState::On);
        assert!(map.is_under_diagnosis(&ChannelRef::from(&key)));
        assert_eq!(map.commanded_true_duty(&ChannelRef::from(&key)), Some(40));
        assert!(map
            .is_under_diagnosis(&ChannelRef::new(&key.0, "fan2"))
            .not());
        assert_eq!(map.len(), 1);
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn replacing_a_known_channels_entry_allocates_nothing() {
        // Goal: only a channel's first entry builds an owned key, so the per-write state update
        // allocates nothing. Method: insert once, then count a second replace and its reads.
        let map = FanStateMap::new();
        let device_uid = "dev-a".to_string();
        let channel = ChannelRef::new(&device_uid, "fan1");
        map.replace(channel, ChannelEntry::default_off());
        let on = ChannelEntry {
            state: FanState::On,
            under_diagnosis: false,
            commanded_true_duty: Some(55),
        };

        let (entry, allocations) = crate::alloc_count::count_allocations(|| {
            map.replace(channel, on);
            map.entry(&channel)
        });

        assert_eq!(entry, on);
        assert_eq!(map.len(), 1);
        assert_eq!(allocations, 0);
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
            (&key).into(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
                commanded_true_duty: None,
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
            (&key).into(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
                commanded_true_duty: None,
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
            (&key).into(),
            ChannelEntry {
                state: FanState::Kicking { sustain_target: 60 },
                under_diagnosis: false,
                commanded_true_duty: None,
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
            (&key).into(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
                commanded_true_duty: None,
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
