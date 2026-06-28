/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
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

//! Single active calibration batch.
//!
//! A batch is an ordered queue of channels the daemon calibrates one at
//! a time. The daemon owning the queue (rather than the UI driving it)
//! is the whole point: a browser suspend/reload never strands the
//! remaining fans, since the spawned driver keeps advancing and the UI
//! just polls status and re-attaches.
//!
//! This module is the pure state machine. The engine owns one instance,
//! the spawned driver mutates phases as it advances, and status polls
//! read snapshots. All of that happens on the single-threaded runtime,
//! so interior mutability is a `RefCell` (mirroring `DiagnosisRegistry`).

use super::ChannelKey;
use crate::device::{ChannelName, DeviceUID};
use chrono::{DateTime, Local};
use std::cell::RefCell;
use std::ops::Not;
use tokio_util::sync::CancellationToken;

/// Upper bound on channels in one batch. A host has far fewer fan
/// channels than this; the cap turns a malformed request into a clean
/// rejection instead of unbounded work.
pub const MAX_BATCH_CHANNELS: usize = 64;
const _: () = assert!(MAX_BATCH_CHANNELS <= u8::MAX as usize);

/// Where a single channel sits in the batch lifecycle. The driver owns
/// these transitions, so a status poll never has to disambiguate a
/// fan's sticky prior calibration from this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchEntryPhase {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl BatchEntryPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone)]
pub struct BatchEntry {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub phase: BatchEntryPhase,
    pub message: Option<String>,
}

/// Immutable clone of the batch for the status endpoint.
#[derive(Debug, Clone)]
pub struct BatchSnapshot {
    pub active: bool,
    pub started_at: DateTime<Local>,
    pub entries: Vec<BatchEntry>,
}

struct Batch {
    entries: Vec<BatchEntry>,
    cancel_token: CancellationToken,
    started_at: DateTime<Local>,
    active: bool,
    /// How many sweeps run at once. 1 is sequential; the driver
    /// processes the queue in groups of this size.
    concurrency: usize,
}

/// Why a batch could not begin. Mapped to a REST error at the boundary.
#[derive(Debug, PartialEq, Eq)]
pub enum BatchBeginError {
    Empty,
    TooManyChannels { requested: usize, max: usize },
    AlreadyActive,
}

/// The single active calibration batch (at most one at a time).
pub struct CalibrationBatchState {
    inner: RefCell<Option<Batch>>,
}

impl Default for CalibrationBatchState {
    fn default() -> Self {
        Self::new()
    }
}

impl CalibrationBatchState {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(None),
        }
    }

    /// Validate and install a new batch. `concurrency` is how many sweeps
    /// run at once (1 = sequential), clamped to `[1, channel count]` so a
    /// bogus value can never schedule more work than there are fans.
    /// Rejects an empty or oversized request, or a second batch while one
    /// is still active. Returns the driver's cancellation token on success.
    pub fn try_begin(
        &self,
        channels: Vec<ChannelKey>,
        concurrency: usize,
    ) -> Result<CancellationToken, BatchBeginError> {
        if channels.is_empty() {
            return Err(BatchBeginError::Empty);
        }
        if channels.len() > MAX_BATCH_CHANNELS {
            return Err(BatchBeginError::TooManyChannels {
                requested: channels.len(),
                max: MAX_BATCH_CHANNELS,
            });
        }
        let mut slot = self.inner.borrow_mut();
        if slot.as_ref().is_some_and(|batch| batch.active) {
            return Err(BatchBeginError::AlreadyActive);
        }
        let entries: Vec<BatchEntry> = channels
            .into_iter()
            .map(|(device_uid, channel_name)| BatchEntry {
                device_uid,
                channel_name,
                phase: BatchEntryPhase::Queued,
                message: None,
            })
            .collect();
        assert!(entries.is_empty().not());
        assert!(entries.len() <= MAX_BATCH_CHANNELS);
        let concurrency = concurrency.clamp(1, entries.len());
        assert!(concurrency >= 1);
        assert!(concurrency <= entries.len());
        let cancel_token = CancellationToken::new();
        *slot = Some(Batch {
            entries,
            cancel_token: cancel_token.clone(),
            started_at: Local::now(),
            active: true,
            concurrency,
        });
        Ok(cancel_token)
    }

    /// Ordered channel keys the driver iterates. `None` if no batch is
    /// installed.
    pub fn entry_keys(&self) -> Option<Vec<ChannelKey>> {
        let slot = self.inner.borrow();
        slot.as_ref().map(|batch| {
            batch
                .entries
                .iter()
                .map(|entry| (entry.device_uid.clone(), entry.channel_name.clone()))
                .collect()
        })
    }

    pub fn is_active(&self) -> bool {
        let slot = self.inner.borrow();
        slot.as_ref().is_some_and(|batch| batch.active)
    }

    pub fn is_cancelled(&self) -> bool {
        let slot = self.inner.borrow();
        slot.as_ref()
            .is_some_and(|batch| batch.cancel_token.is_cancelled())
    }

    /// How many sweeps the driver runs at once. 1 (sequential) when no
    /// batch is installed.
    pub fn concurrency(&self) -> usize {
        let slot = self.inner.borrow();
        slot.as_ref().map_or(1, |batch| batch.concurrency)
    }

    /// Move one entry to `Running`. The index comes from the driver's own
    /// loop over `entry_keys`, so it is always in range.
    pub fn mark_running(&self, index: usize) {
        let mut slot = self.inner.borrow_mut();
        let Some(batch) = slot.as_mut() else {
            return;
        };
        assert!(index < batch.entries.len());
        batch.entries[index].phase = BatchEntryPhase::Running;
    }

    /// Move one entry to a terminal phase with an optional failure
    /// message.
    pub fn mark_terminal(&self, index: usize, phase: BatchEntryPhase, message: Option<String>) {
        assert!(phase.is_terminal());
        let mut slot = self.inner.borrow_mut();
        let Some(batch) = slot.as_mut() else {
            return;
        };
        assert!(index < batch.entries.len());
        batch.entries[index].phase = phase;
        batch.entries[index].message = message;
    }

    /// Mark every still-pending entry from `index` onward as cancelled,
    /// so a tripped batch does not run its tail.
    pub fn mark_remaining_cancelled(&self, index: usize) {
        let mut slot = self.inner.borrow_mut();
        let Some(batch) = slot.as_mut() else {
            return;
        };
        assert!(index <= batch.entries.len());
        for entry in batch.entries.iter_mut().skip(index) {
            if entry.phase.is_terminal().not() {
                entry.phase = BatchEntryPhase::Cancelled;
            }
        }
    }

    /// Flip the batch inactive once the driver finishes. The entries stay
    /// readable so a final status poll shows the outcome.
    pub fn set_inactive(&self) {
        let mut slot = self.inner.borrow_mut();
        if let Some(batch) = slot.as_mut() {
            batch.active = false;
        }
    }

    /// Trip the cancel token and report every channel currently running
    /// (a concurrent group can have several) so the engine can interrupt
    /// each in-flight sweep. Empty when no batch is active or none is
    /// running yet (cancel between groups).
    pub fn cancel(&self) -> Vec<ChannelKey> {
        let slot = self.inner.borrow();
        let Some(batch) = slot.as_ref() else {
            return Vec::new();
        };
        if batch.active.not() {
            return Vec::new();
        }
        batch.cancel_token.cancel();
        batch
            .entries
            .iter()
            .filter(|entry| entry.phase == BatchEntryPhase::Running)
            .map(|entry| (entry.device_uid.clone(), entry.channel_name.clone()))
            .collect()
    }

    /// Clone the current batch for the status endpoint. `None` when no
    /// batch has started this session.
    pub fn snapshot(&self) -> Option<BatchSnapshot> {
        let slot = self.inner.borrow();
        slot.as_ref().map(|batch| BatchSnapshot {
            active: batch.active,
            started_at: batch.started_at,
            entries: batch.entries.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(device: &str, channel: &str) -> ChannelKey {
        (device.to_string(), channel.to_string())
    }

    #[test]
    fn try_begin_installs_active_batch_with_queued_entries() {
        // Goal: a valid begin stores an active batch whose entries all
        // start Queued, ready for the driver to advance.
        let state = CalibrationBatchState::new();
        let token = state
            .try_begin(vec![key("d", "fan1"), key("d", "fan2")], 1)
            .expect("begin");
        assert!(token.is_cancelled().not());
        let snapshot = state.snapshot().expect("snapshot");
        assert!(snapshot.active);
        assert_eq!(snapshot.entries.len(), 2);
        assert!(snapshot
            .entries
            .iter()
            .all(|entry| entry.phase == BatchEntryPhase::Queued));
    }

    #[test]
    fn try_begin_rejects_empty() {
        // Goal: an empty channel list is malformed and refused before
        // any state is installed.
        let state = CalibrationBatchState::new();
        assert_eq!(
            state.try_begin(vec![], 1).unwrap_err(),
            BatchBeginError::Empty
        );
        assert!(state.snapshot().is_none());
    }

    #[test]
    fn try_begin_rejects_over_max() {
        // Goal: a request beyond the hard cap is refused rather than
        // scheduling unbounded work.
        let state = CalibrationBatchState::new();
        let channels: Vec<ChannelKey> = (0..=MAX_BATCH_CHANNELS)
            .map(|i| key("d", &format!("fan{i}")))
            .collect();
        assert_eq!(channels.len(), MAX_BATCH_CHANNELS + 1);
        let err = state.try_begin(channels, 1).unwrap_err();
        assert!(matches!(err, BatchBeginError::TooManyChannels { .. }));
    }

    #[test]
    fn try_begin_rejects_second_while_active() {
        // Goal: only one batch may be active at a time.
        let state = CalibrationBatchState::new();
        state.try_begin(vec![key("d", "fan1")], 1).expect("first");
        assert_eq!(
            state.try_begin(vec![key("d", "fan2")], 1).unwrap_err(),
            BatchBeginError::AlreadyActive
        );
    }

    #[test]
    fn try_begin_allowed_after_previous_inactive() {
        // Goal: once a batch finishes (inactive), a fresh one can begin.
        let state = CalibrationBatchState::new();
        state.try_begin(vec![key("d", "fan1")], 1).expect("first");
        state.set_inactive();
        state.try_begin(vec![key("d", "fan2")], 1).expect("second");
        let snapshot = state.snapshot().expect("snapshot");
        assert!(snapshot.active);
        assert_eq!(snapshot.entries[0].channel_name, "fan2");
    }

    #[test]
    fn mark_running_then_terminal_records_phase_and_message() {
        // Goal: the driver's phase transitions land on the right entry
        // and a failure message is preserved for the UI.
        let state = CalibrationBatchState::new();
        state
            .try_begin(vec![key("d", "fan1"), key("d", "fan2")], 1)
            .expect("begin");
        state.mark_running(0);
        assert_eq!(
            state.snapshot().unwrap().entries[0].phase,
            BatchEntryPhase::Running
        );
        state.mark_terminal(0, BatchEntryPhase::Done, None);
        state.mark_running(1);
        state.mark_terminal(1, BatchEntryPhase::Failed, Some("boom".to_string()));
        let snapshot = state.snapshot().unwrap();
        assert_eq!(snapshot.entries[0].phase, BatchEntryPhase::Done);
        assert_eq!(snapshot.entries[1].phase, BatchEntryPhase::Failed);
        assert_eq!(snapshot.entries[1].message.as_deref(), Some("boom"));
    }

    #[test]
    fn mark_remaining_cancelled_skips_terminal_entries() {
        // Goal: once cancelled, only still-pending entries flip to
        // Cancelled; an already-finished entry keeps its outcome.
        let state = CalibrationBatchState::new();
        state
            .try_begin(vec![key("d", "a"), key("d", "b"), key("d", "c")], 1)
            .expect("begin");
        state.mark_terminal(0, BatchEntryPhase::Done, None);
        state.mark_remaining_cancelled(1);
        let snapshot = state.snapshot().unwrap();
        assert_eq!(snapshot.entries[0].phase, BatchEntryPhase::Done);
        assert_eq!(snapshot.entries[1].phase, BatchEntryPhase::Cancelled);
        assert_eq!(snapshot.entries[2].phase, BatchEntryPhase::Cancelled);
    }

    #[test]
    fn cancel_trips_token_and_reports_running_channel() {
        // Goal: cancelling an active batch trips its token (so the driver
        // stops) and names the in-flight channel so the engine can
        // interrupt that sweep.
        let state = CalibrationBatchState::new();
        let token = state
            .try_begin(vec![key("d", "fan1"), key("d", "fan2")], 1)
            .expect("begin");
        state.mark_running(1);
        let running = state.cancel();
        assert_eq!(running, vec![key("d", "fan2")]);
        assert!(token.is_cancelled());
        assert!(state.is_cancelled());
    }

    #[test]
    fn cancel_without_running_entry_returns_empty_but_trips_token() {
        // Goal: cancelling between groups (none Running) still trips the
        // token so the queue tail does not start.
        let state = CalibrationBatchState::new();
        let token = state.try_begin(vec![key("d", "fan1")], 1).expect("begin");
        let running = state.cancel();
        assert!(running.is_empty());
        assert!(token.is_cancelled());
    }

    #[test]
    fn try_begin_clamps_concurrency_to_channel_count() {
        // Goal: concurrency is clamped to [1, channel count]. A 0 (or
        // omitted) value becomes 1 (sequential); an over-large value
        // becomes "all selected at once".
        let state = CalibrationBatchState::new();
        state
            .try_begin(vec![key("d", "a"), key("d", "b")], 0)
            .expect("begin");
        assert_eq!(state.concurrency(), 1);
        state.set_inactive();
        state
            .try_begin(vec![key("d", "a"), key("d", "b")], 9)
            .expect("begin");
        assert_eq!(state.concurrency(), 2);
    }

    #[test]
    fn snapshot_none_before_any_batch() {
        // Goal: status is absent until the first batch begins.
        let state = CalibrationBatchState::new();
        assert!(state.snapshot().is_none());
        assert!(state.is_active().not());
        assert!(state.is_cancelled().not());
        assert_eq!(state.concurrency(), 1);
    }
}
