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

//! Unified write entry point for calibrated channels.
//!
//! Every duty write made on behalf of an engine commander or an API
//! setting actor goes through [`dispatch`], which:
//!
//! - **Passthrough**: for uncalibrated channels, stepped calibrations,
//!   and channels currently under diagnosis, writes the raw user
//!   value to hardware and returns.
//! - **Kick-then-settle orchestration**: for smooth calibrations, a
//!   transition out of `Off` writes the kick duty and schedules a
//!   deferred sustain-write task via `tokio::task::spawn_local`.
//! - **Mid-kick target adjustment**: subsequent dispatches that arrive
//!   while the channel is still `Kicking` only update the pending
//!   sustain target; they do not re-kick.
//!
//! The spawn path uses `tokio::task::spawn_local` (not a
//! `moro_local::Scope`) because `dispatch` is called from inside
//! already-spawned futures on the engine's main scope, and
//! `moro_local::Scope::spawn` panics with `RefCell already borrowed`
//! when re-entered from inside an active scope task. The clone cost
//! here is one `Rc::clone` of `state` and `writer` per Off->Kicking
//! transition, which is rare (only when a fan goes from stopped to
//! spinning, not every per-tick write).
//!
//! Tests call `complete_kick` directly to exercise the post-kick
//! transition without driving an async timer.

#![allow(dead_code)]
// `state` / `store` / `writer` / `writes` are well-established semantic
// names in this module (the state map, the calibration store, the duty
// writer, and the captured write log). Renaming any of them to satisfy
// pedantic similar_names would hurt readability.
#![allow(clippy::similar_names)]

use super::curve::MappedDuty;
use super::state::{ChannelEntry, FanState, FanStateMap};
use super::store::CalibrationStore;
use super::ChannelKey;
use crate::device::{ChannelName, DeviceUID, Duty, UID};
use crate::repositories::repository::Repository;
use anyhow::Result;
use async_trait::async_trait;
use log::warn;
use std::rc::Rc;
use std::time::Duration;

/// Hardware-write abstraction the dispatcher targets.
///
/// Production code wraps `Rc<dyn Repository>` in `RepoWriter` (below).
/// Tests implement this trait directly to capture the sequence of writes.
#[async_trait(?Send)]
pub trait DutyWriter {
    async fn write_device_duty(
        &self,
        device_uid: &UID,
        channel_name: &str,
        device_duty: Duty,
    ) -> Result<()>;
}

/// Production adapter: turns a `Repository` into a `DutyWriter` by
/// delegating to `apply_setting_speed_fixed`.
///
/// Constructed at every dispatch call site (engine commander, manual
/// setting path). Tiny and cheap.
pub struct RepoWriter {
    repo: Rc<dyn Repository>,
}

impl RepoWriter {
    pub fn new(repo: Rc<dyn Repository>) -> Self {
        Self { repo }
    }

    /// Convenience: returns an `Rc<dyn DutyWriter>` ready to hand to
    /// `dispatch`. Call sites typically use this form.
    pub fn rc(repo: Rc<dyn Repository>) -> Rc<dyn DutyWriter> {
        Rc::new(Self::new(repo))
    }
}

#[async_trait(?Send)]
impl DutyWriter for RepoWriter {
    async fn write_device_duty(
        &self,
        device_uid: &UID,
        channel_name: &str,
        device_duty: Duty,
    ) -> Result<()> {
        self.repo
            .apply_setting_speed_fixed(device_uid, channel_name, device_duty)
            .await
    }
}

/// Outcome of the synchronous part of a dispatch. Returned by
/// `dispatch_core`; the public `dispatch` entry point uses it to
/// decide whether to schedule a deferred sustain write.
enum DispatchOutcome {
    /// All work completed; no follow-up needed.
    Done,
    /// The kick was written and the channel is now `Kicking`. A
    /// deferred task must, after `kick_duration_ms`, call
    /// `complete_kick(state, writer, &key, &device_uid, &channel_name)`.
    SustainPending {
        kick_duration_ms: u32,
        key: ChannelKey,
        device_uid: DeviceUID,
        channel_name: ChannelName,
    },
}

/// Apply a user-facing `true_duty` to the channel.
///
/// On a smooth-curve channel transitioning out of `Off`, the kick
/// duty is written immediately and a deferred sustain-write task is
/// spawned via `tokio::task::spawn_local`. The clone of `state` and
/// `writer` into the spawned task only happens on this rare
/// transition; the hot per-tick paths (mid-kick update, On-state
/// write, passthrough for uncalibrated/stepped channels) do zero
/// clones.
///
/// `moro_local::Scope::spawn` is intentionally not used here even
/// though the main loop has a scope available: `dispatch` is called
/// from inside futures that are themselves already spawned on the
/// main scope, and re-entering `scope.spawn` from inside an active
/// scope task panics with `RefCell already borrowed` in
/// `moro-local 0.4.0`. `tokio::task::spawn_local` has no such
/// restriction.
pub async fn dispatch(
    state: &Rc<FanStateMap>,
    store: &CalibrationStore,
    writer: &Rc<dyn DutyWriter>,
    device_uid: DeviceUID,
    channel_name: ChannelName,
    true_duty: Duty,
) -> Result<()> {
    let outcome = dispatch_core(state, store, writer, device_uid, channel_name, true_duty).await?;
    if let DispatchOutcome::SustainPending {
        kick_duration_ms,
        key,
        device_uid,
        channel_name,
    } = outcome
    {
        let state_owned = Rc::clone(state);
        let writer_owned = Rc::clone(writer);
        tokio::task::spawn_local(async move {
            tokio::time::sleep(Duration::from_millis(u64::from(kick_duration_ms))).await;
            complete_kick(
                &state_owned,
                &writer_owned,
                &key,
                &device_uid,
                &channel_name,
            )
            .await;
        });
    }
    Ok(())
}

/// Synchronous dispatch logic. Performs the immediate hardware write
/// (passthrough, write-zero, kick, or sustain-on-On) and updates the
/// per-channel state machine. Returns `SustainPending` only on the
/// `Off -> Kicking` transition so the caller can schedule the
/// deferred sustain write.
async fn dispatch_core(
    state: &Rc<FanStateMap>,
    store: &CalibrationStore,
    writer: &Rc<dyn DutyWriter>,
    device_uid: DeviceUID,
    channel_name: ChannelName,
    true_duty: Duty,
) -> Result<DispatchOutcome> {
    let key: ChannelKey = (device_uid.clone(), channel_name.clone());

    if state.is_under_diagnosis(&key) {
        return Ok(DispatchOutcome::Done);
    }

    let calibration = store.get(&key);
    let mapping = calibration
        .as_ref()
        .and_then(|cal| cal.true_to_device(true_duty));
    let Some(mapped) = mapping else {
        writer
            .write_device_duty(&device_uid, &channel_name, true_duty)
            .await?;
        return Ok(DispatchOutcome::Done);
    };
    let kick_duration_ms = calibration.as_ref().map_or(0, |cal| cal.kick_duration_ms);

    if true_duty == 0 {
        handle_write_zero(state, writer, key, device_uid, channel_name).await?;
        return Ok(DispatchOutcome::Done);
    }

    let entry = state.entry(&key);
    match entry.state {
        FanState::Off => {
            start_kick(
                state,
                writer,
                &key,
                entry,
                mapped,
                &device_uid,
                &channel_name,
            )
            .await?;
            Ok(DispatchOutcome::SustainPending {
                kick_duration_ms,
                key,
                device_uid,
                channel_name,
            })
        }
        FanState::Kicking { .. } => {
            update_kick_target(state, key, entry, mapped.sustain);
            Ok(DispatchOutcome::Done)
        }
        FanState::On => {
            write_sustain_on(state, writer, key, entry, mapped, device_uid, channel_name).await?;
            Ok(DispatchOutcome::Done)
        }
    }
}

/// Transition any state to `Off` and write 0 to hardware.
async fn handle_write_zero(
    state: &Rc<FanStateMap>,
    writer: &Rc<dyn DutyWriter>,
    key: ChannelKey,
    device_uid: DeviceUID,
    channel_name: ChannelName,
) -> Result<()> {
    let prior = state.entry(&key);
    state.replace(
        key,
        ChannelEntry {
            state: FanState::Off,
            under_diagnosis: prior.under_diagnosis,
        },
    );
    writer
        .write_device_duty(&device_uid, &channel_name, 0)
        .await
}

/// Start a kick: set state to `Kicking` and write `kick_duty`. Returns
/// to the caller, which schedules the deferred sustain write. No clones
/// or spawns inside this helper.
async fn start_kick(
    state: &Rc<FanStateMap>,
    writer: &Rc<dyn DutyWriter>,
    key: &ChannelKey,
    prior: ChannelEntry,
    mapped: MappedDuty,
    device_uid: &UID,
    channel_name: &str,
) -> Result<()> {
    state.replace(
        key.clone(),
        ChannelEntry {
            state: FanState::Kicking {
                sustain_target: mapped.sustain,
            },
            under_diagnosis: prior.under_diagnosis,
        },
    );
    writer
        .write_device_duty(device_uid, channel_name, mapped.kick)
        .await
}

/// While `Kicking`, only the pending sustain target is updated; the
/// deferred task will pick it up when the kick window elapses. Avoids
/// re-kicking when the user oscillates the target mid-kick.
fn update_kick_target(
    state: &Rc<FanStateMap>,
    key: ChannelKey,
    prior: ChannelEntry,
    new_sustain: Duty,
) {
    state.replace(
        key,
        ChannelEntry {
            state: FanState::Kicking {
                sustain_target: new_sustain,
            },
            under_diagnosis: prior.under_diagnosis,
        },
    );
}

/// When the channel is `On`, a new dispatch simply writes the new
/// sustain duty. State stays `On`.
async fn write_sustain_on(
    state: &Rc<FanStateMap>,
    writer: &Rc<dyn DutyWriter>,
    key: ChannelKey,
    prior: ChannelEntry,
    mapped: MappedDuty,
    device_uid: DeviceUID,
    channel_name: ChannelName,
) -> Result<()> {
    state.replace(
        key,
        ChannelEntry {
            state: FanState::On,
            under_diagnosis: prior.under_diagnosis,
        },
    );
    writer
        .write_device_duty(&device_uid, &channel_name, mapped.sustain)
        .await
}

/// Finalize a pending kick: if the channel is still in `Kicking`,
/// transition to `On` and write the latest `sustain_target`. Called
/// by the spawned task in production and directly by unit tests.
pub async fn complete_kick(
    state: &Rc<FanStateMap>,
    writer: &Rc<dyn DutyWriter>,
    key: &ChannelKey,
    device_uid: &UID,
    channel_name: &str,
) {
    let target = {
        let entry = state.entry(key);
        match entry.state {
            FanState::Kicking { sustain_target } => {
                state.replace(
                    key.clone(),
                    ChannelEntry {
                        state: FanState::On,
                        under_diagnosis: entry.under_diagnosis,
                    },
                );
                Some(sustain_target)
            }
            FanState::Off | FanState::On => None,
        }
    };
    if let Some(duty) = target {
        if let Err(err) = writer
            .write_device_duty(device_uid, channel_name, duty)
            .await
        {
            warn!("Calibration sustain write failed for {device_uid}:{channel_name} - {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::curve::{Calibration, CurveKind, DutySample};
    use super::*;
    use crate::device::RPM;
    use chrono::Local;
    use std::cell::RefCell;

    /// Build a uniform 5%-step `Vec<DutySample>` curve from a closure
    /// that maps each duty index (0..21) to an RPM value.
    fn build_curve<F: FnMut(usize) -> RPM>(mut rpm_at: F) -> Vec<DutySample> {
        (0..21usize)
            .map(|i| DutySample {
                duty: u8::try_from(i).expect("fits in u8") * 5,
                rpm: rpm_at(i),
            })
            .collect()
    }

    /// Test writer that captures every (uid, channel, duty) write in
    /// order. Cheap to clone via `Rc` so the test keeps a handle to
    /// inspect after invoking dispatch.
    struct MockWriter {
        writes: Rc<RefCell<Vec<(String, String, Duty)>>>,
        fail_next: Rc<RefCell<bool>>,
    }

    impl MockWriter {
        /// Returns the writer as the trait-object Rc the dispatch
        /// signature expects, plus a handle to the captured writes.
        fn make() -> (Rc<dyn DutyWriter>, Rc<RefCell<Vec<(String, String, Duty)>>>) {
            let writes = Rc::new(RefCell::new(Vec::new()));
            let writer: Rc<dyn DutyWriter> = Rc::new(Self {
                writes: Rc::clone(&writes),
                fail_next: Rc::new(RefCell::new(false)),
            });
            (writer, writes)
        }
    }

    #[async_trait(?Send)]
    impl DutyWriter for MockWriter {
        async fn write_device_duty(
            &self,
            device_uid: &UID,
            channel_name: &str,
            device_duty: Duty,
        ) -> Result<()> {
            if *self.fail_next.borrow() {
                *self.fail_next.borrow_mut() = false;
                return Err(anyhow::anyhow!("simulated failure"));
            }
            self.writes.borrow_mut().push((
                device_uid.clone(),
                channel_name.to_string(),
                device_duty,
            ));
            Ok(())
        }
    }

    fn k(dev: &str, chan: &str) -> ChannelKey {
        (dev.to_string(), chan.to_string())
    }

    /// Build a smooth calibration with predictable values:
    /// `min_start_duty=5`, `rpm_max=2000`, `kick_duration_ms=500`.
    fn smooth_cal() -> Calibration {
        let up = build_curve(|i| 100 * u32::try_from(i).expect("fits in u32"));
        let down = up.clone();
        Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 500,
            min_start_duty: 5,
            min_sustain_duty: 5,
            min_stable_duty: 5,
            max_eff_duty: 95,
            rpm_max: 2000,
            curve_kind: CurveKind::Smooth,
            warnings: Vec::new(),
            timestamp: Local::now(),
        }
    }

    fn stepped_cal() -> Calibration {
        let up = build_curve(|i| {
            if i < 5 {
                0
            } else if i < 13 {
                1000
            } else {
                2000
            }
        });
        let down = up.clone();
        Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 0,
            min_start_duty: 25,
            min_sustain_duty: 25,
            min_stable_duty: 25,
            max_eff_duty: 65,
            rpm_max: 2000,
            curve_kind: CurveKind::Stepped,
            warnings: Vec::new(),
            timestamp: Local::now(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn uncalibrated_channel_passes_through() {
        // Goal: with no calibration stored, dispatch must write the
        // user's true_duty value verbatim. This is the path most users
        // start on before opting into calibration.
        let state = Rc::new(FanStateMap::new());
        let store = CalibrationStore::empty();
        let (writer, writes) = MockWriter::make();
        dispatch(
            &state,
            &store,
            &writer,
            "dev-a".to_string(),
            "fan1".to_string(),
            42,
        )
        .await
        .expect("ok");
        assert_eq!(
            writes.borrow().as_slice(),
            &[("dev-a".to_string(), "fan1".to_string(), 42)]
        );
        assert_eq!(state.entry(&k("dev-a", "fan1")).state, FanState::Off);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stepped_calibration_passes_through() {
        // Goal: a stepped calibration must keep mapping disabled so
        // the user's value reaches the hardware unchanged. The state
        // machine must not transition into Kicking.
        let state = Rc::new(FanStateMap::new());
        let store = CalibrationStore::empty();
        store.insert_unsaved(k("dev-a", "fan1"), stepped_cal());
        let (writer, writes) = MockWriter::make();
        dispatch(
            &state,
            &store,
            &writer,
            "dev-a".to_string(),
            "fan1".to_string(),
            42,
        )
        .await
        .expect("ok");
        assert_eq!(writes.borrow().len(), 1);
        assert_eq!(writes.borrow()[0].2, 42);
        assert_eq!(state.entry(&k("dev-a", "fan1")).state, FanState::Off);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn channel_under_diagnosis_is_a_noop() {
        // Goal: while the diagnoser owns the channel, dispatch must do
        // nothing. The diagnoser writes directly to the repo so engine
        // ticks would otherwise stomp the sweep.
        let state = Rc::new(FanStateMap::new());
        state.set_under_diagnosis(k("dev-a", "fan1"), true);
        let store = CalibrationStore::empty();
        store.insert_unsaved(k("dev-a", "fan1"), smooth_cal());
        let (writer, writes) = MockWriter::make();
        dispatch(
            &state,
            &store,
            &writer,
            "dev-a".to_string(),
            "fan1".to_string(),
            42,
        )
        .await
        .expect("ok");
        assert!(writes.borrow().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn off_to_kicking_writes_kick_duty_and_updates_state() {
        // Goal: a calibrated smooth channel transitioning out of Off
        // must (1) write the kick duty to hardware, (2) record the
        // intended sustain duty in the state map for the deferred
        // task to consume.
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let state = Rc::new(FanStateMap::new());
                let store = CalibrationStore::empty();
                store.insert_unsaved(k("dev-a", "fan1"), smooth_cal());
                let (writer, writes) = MockWriter::make();
                dispatch(
                    &state,
                    &store,
                    &writer,
                    "dev-a".to_string(),
                    "fan1".to_string(),
                    50,
                )
                .await
                .expect("ok");
                // One write so far: the kick. The sustain write is
                // scheduled via spawn_local; we never drive the timer in
                // this test, so the spawned future never fires before
                // the LocalSet is dropped.
                assert_eq!(writes.borrow().len(), 1);
                let kick_written = writes.borrow()[0].2;
                let mapped = smooth_cal().true_to_device(50).expect("smooth");
                assert_eq!(kick_written, mapped.kick);
                let entry = state.entry(&k("dev-a", "fan1"));
                assert_eq!(
                    entry.state,
                    FanState::Kicking {
                        sustain_target: mapped.sustain
                    }
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn write_zero_from_kicking_returns_to_off() {
        // Goal: a user-cancel (true_duty=0) while a kick is in flight
        // must immediately write 0 and set state to Off. The deferred
        // task, when it eventually fires, must observe Off and skip.
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let state = Rc::new(FanStateMap::new());
                let store = CalibrationStore::empty();
                store.insert_unsaved(k("dev-a", "fan1"), smooth_cal());
                let (writer, writes) = MockWriter::make();
                dispatch(
                    &state,
                    &store,
                    &writer,
                    "dev-a".to_string(),
                    "fan1".to_string(),
                    50,
                )
                .await
                .expect("kick");
                dispatch(
                    &state,
                    &store,
                    &writer,
                    "dev-a".to_string(),
                    "fan1".to_string(),
                    0,
                )
                .await
                .expect("zero");
                let log = writes.borrow().clone();
                assert_eq!(log.len(), 2);
                assert_eq!(log[1].2, 0);
                assert_eq!(state.entry(&k("dev-a", "fan1")).state, FanState::Off);
                // Now finalize the kick manually; should be a no-op
                // because state has moved to Off.
                complete_kick(
                    &state,
                    &writer,
                    &k("dev-a", "fan1"),
                    &"dev-a".to_string(),
                    "fan1",
                )
                .await;
                assert_eq!(writes.borrow().len(), 2);
                assert_eq!(state.entry(&k("dev-a", "fan1")).state, FanState::Off);
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn mid_kick_dispatch_updates_sustain_target_without_writing() {
        // Goal: while the fan is mid-kick, a new dispatch only updates
        // the pending sustain target. No hardware write happens until
        // the deferred task fires; then the latest sustain wins.
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let state = Rc::new(FanStateMap::new());
                let store = CalibrationStore::empty();
                store.insert_unsaved(k("dev-a", "fan1"), smooth_cal());
                let (writer, writes) = MockWriter::make();
                let cal = smooth_cal();
                dispatch(
                    &state,
                    &store,
                    &writer,
                    "dev-a".to_string(),
                    "fan1".to_string(),
                    50,
                )
                .await
                .expect("kick");
                assert_eq!(writes.borrow().len(), 1);
                dispatch(
                    &state,
                    &store,
                    &writer,
                    "dev-a".to_string(),
                    "fan1".to_string(),
                    70,
                )
                .await
                .expect("mid-kick update");
                assert_eq!(writes.borrow().len(), 1, "no extra hardware write");
                let expected_sustain = cal.true_to_device(70).expect("smooth").sustain;
                assert_eq!(
                    state.entry(&k("dev-a", "fan1")).state,
                    FanState::Kicking {
                        sustain_target: expected_sustain
                    }
                );
                // Finalize manually: now the second sustain target
                // should be written, state becomes On.
                complete_kick(
                    &state,
                    &writer,
                    &k("dev-a", "fan1"),
                    &"dev-a".to_string(),
                    "fan1",
                )
                .await;
                assert_eq!(writes.borrow().len(), 2);
                assert_eq!(writes.borrow()[1].2, expected_sustain);
                assert_eq!(state.entry(&k("dev-a", "fan1")).state, FanState::On);
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn on_state_writes_sustain_directly() {
        // Goal: once the channel is On, subsequent dispatches go
        // straight to the new sustain duty with no kick. The deferred
        // task does not run because the state machine never re-entered
        // Off.
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let state = Rc::new(FanStateMap::new());
                let key = k("dev-a", "fan1");
                state.replace(
                    key.clone(),
                    ChannelEntry {
                        state: FanState::On,
                        under_diagnosis: false,
                    },
                );
                let store = CalibrationStore::empty();
                store.insert_unsaved(key.clone(), smooth_cal());
                let (writer, writes) = MockWriter::make();
                dispatch(
                    &state,
                    &store,
                    &writer,
                    "dev-a".to_string(),
                    "fan1".to_string(),
                    80,
                )
                .await
                .expect("ok");
                let expected = smooth_cal().true_to_device(80).expect("smooth").sustain;
                assert_eq!(writes.borrow().len(), 1);
                assert_eq!(writes.borrow()[0].2, expected);
                assert_eq!(state.entry(&key).state, FanState::On);
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn complete_kick_on_off_state_is_noop() {
        // Goal: complete_kick must defensively no-op if the state is
        // already Off or On (e.g. a stale spawned task whose channel
        // got reset). Prevents the task from overwriting a user-set
        // zero with a stale sustain.
        let state = Rc::new(FanStateMap::new());
        let key = k("dev-a", "fan1");
        state.replace(
            key.clone(),
            ChannelEntry {
                state: FanState::Off,
                under_diagnosis: false,
            },
        );
        let (writer, writes) = MockWriter::make();
        complete_kick(&state, &writer, &key, &"dev-a".to_string(), "fan1").await;
        assert!(writes.borrow().is_empty());
        assert_eq!(state.entry(&key).state, FanState::Off);
    }
}
