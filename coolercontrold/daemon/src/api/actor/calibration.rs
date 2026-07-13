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

use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::calibration::CalibrationView;
use crate::calibration::{
    Calibration, CalibrationEntry, ChannelKey, DiagnosisFailure, DiagnosisPhase, DiagnosisProgress,
};
use crate::device::{ChannelName, DeviceUID};
use crate::engine::main::Engine;
use anyhow::Result;
use chrono::{DateTime, Local};
use moro_local::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Bounded queue depth for the calibration actor mailbox. Sized for
/// the worst expected pile-up of concurrent REST handlers (one start,
/// one cancel, the UI poll for in-progress / status, plus a couple of
/// `get_all` calls during a multi-tab reload) with margin. When full,
/// `Sender::send` awaits and the request thread's response slows
/// rather than dropping the message.
const CALIBRATION_ACTOR_QUEUE_DEPTH: usize = 16;

/// Per-channel status of the latest calibration attempt. The UI polls
/// `GET /calibrations/.../status` to drive its progress UI; the engine
/// rewrites this entry on every sweep step and once more on the final
/// terminal transition. The `Completed` and `Failed` variants stick
/// around until the next diagnosis on the same channel resets it back
/// to `InProgress`. `NotStarted` is returned when neither an in-memory
/// snapshot nor a persisted calibration exists for the channel; the
/// endpoint returns 200 in that case so the UI does not have to
/// distinguish "channel unknown" from network errors via HTTP 404.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum CalibrationStatus {
    NotStarted {
        device_uid: DeviceUID,
        channel_name: ChannelName,
    },
    InProgress {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        stage: String,
        percent: u8,
        current_duty: Option<u8>,
        current_rpm: Option<u32>,
        updated_at: DateTime<Local>,
    },
    Completed {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        completed_at: DateTime<Local>,
        calibration: CalibrationView,
    },
    Failed {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        failed_at: DateTime<Local>,
        reason: String,
        message: String,
    },
}

impl CalibrationStatus {
    pub fn not_started(device_uid: DeviceUID, channel_name: ChannelName) -> Self {
        Self::NotStarted {
            device_uid,
            channel_name,
        }
    }
}

impl CalibrationStatus {
    /// Construct an `InProgress` entry from a diagnoser progress event.
    pub fn from_progress(progress: DiagnosisProgress) -> Self {
        Self::InProgress {
            device_uid: progress.device_uid,
            channel_name: progress.channel_name,
            stage: stage_name(progress.phase).to_string(),
            percent: progress.percent,
            current_duty: progress.current_duty,
            current_rpm: progress.current_rpm,
            updated_at: Local::now(),
        }
    }

    /// Construct a `Completed` entry from a successful sweep result.
    pub fn from_completion(
        device_uid: DeviceUID,
        channel_name: ChannelName,
        calibration: Calibration,
    ) -> Self {
        Self::Completed {
            device_uid,
            channel_name,
            completed_at: Local::now(),
            calibration: calibration.into(),
        }
    }

    /// Construct a `Failed` entry from a sweep failure.
    pub fn from_failure(
        device_uid: DeviceUID,
        channel_name: ChannelName,
        failure: &DiagnosisFailure,
    ) -> Self {
        let (reason, message) = match failure {
            DiagnosisFailure::PreflightTempTooHigh {
                observed,
                limit,
                sensor,
            } => (
                "preflight_temp_too_high",
                format!("{sensor} at {observed:.1} C exceeded the {limit:.1} C pre-flight limit"),
            ),
            DiagnosisFailure::TempAbortedAt {
                observed,
                limit,
                sensor,
            } => (
                "temp_aborted",
                format!(
                    "{sensor} reached {observed:.1} C, over the {limit:.1} C abort limit mid-sweep"
                ),
            ),
            DiagnosisFailure::Cancelled => ("user_cancelled", "diagnosis cancelled".to_string()),
            DiagnosisFailure::WriteFailed(err) => ("write_failed", err.clone()),
            DiagnosisFailure::RestoreFailed(err) => ("restore_failed", err.clone()),
            DiagnosisFailure::PersistFailed(err) => ("persist_failed", err.clone()),
        };
        Self::Failed {
            device_uid,
            channel_name,
            failed_at: Local::now(),
            reason: reason.to_string(),
            message,
        }
    }
}

fn stage_name(phase: DiagnosisPhase) -> &'static str {
    match phase {
        DiagnosisPhase::Preflight => "preflight",
        DiagnosisPhase::UpSweep => "up_sweep",
        DiagnosisPhase::DownSweep => "down_sweep",
        DiagnosisPhase::Finalizing => "finalizing",
    }
}

/// Status of the single active calibration batch. The batch driver owns
/// each entry's `phase`, so the UI renders progress straight from this
/// without disambiguating a fan's sticky prior calibration. The running
/// entry carries live `percent`/`stage` lifted from its per-channel
/// status; a failed entry carries the `message`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationBatchStatus {
    pub active: bool,
    pub started_at: DateTime<Local>,
    pub entries: Vec<CalibrationBatchEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationBatchEntry {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    /// One of: queued, running, done, failed, cancelled.
    pub phase: String,
    pub percent: Option<u8>,
    pub stage: Option<String>,
    pub message: Option<String>,
}

struct CalibrationActor {
    receiver: mpsc::Receiver<CalibrationMessage>,
    engine: Rc<Engine>,
}

pub enum CalibrationMessage {
    Start {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Cancel {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<bool>,
    },
    Get {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<Option<Calibration>>,
    },
    GetAll {
        respond_to: oneshot::Sender<Vec<CalibrationEntry>>,
    },
    Delete {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<Result<bool>>,
    },
    SetOverrides {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        kick_boost_override: Option<bool>,
        kick_duration_override_ms: Option<u32>,
        walk_after_kick_override: Option<bool>,
        respond_to: oneshot::Sender<Result<Option<Calibration>>>,
    },
    InProgress {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<bool>,
    },
    Status {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<CalibrationStatus>,
    },
    StartBatch {
        channels: Vec<ChannelKey>,
        concurrency: usize,
        respond_to: oneshot::Sender<Result<()>>,
    },
    CancelBatch {
        respond_to: oneshot::Sender<bool>,
    },
    BatchStatus {
        respond_to: oneshot::Sender<Option<CalibrationBatchStatus>>,
    },
}

impl CalibrationActor {
    fn new(receiver: mpsc::Receiver<CalibrationMessage>, engine: Rc<Engine>) -> Self {
        Self { receiver, engine }
    }

    /// Reject a duplicate sweep, else spawn the per-channel diagnosis off
    /// the mailbox so a minutes-long sweep does not block it.
    fn spawn_diagnosis(&self, device_uid: DeviceUID, channel_name: ChannelName) -> Result<()> {
        let key: ChannelKey = (device_uid.clone(), channel_name.clone());
        if self.engine.is_calibration_in_progress(&key) {
            return Err(anyhow::anyhow!(
                "calibration already in progress for {device_uid}:{channel_name}"
            ));
        }
        let engine = Rc::clone(&self.engine);
        crate::rt::spawn(async move {
            let _ = engine
                .start_calibration_diagnosis(device_uid, channel_name)
                .await;
        });
        Ok(())
    }

    /// Install the batch synchronously so the caller gets an immediate
    /// conflict, then spawn the long-running driver off the mailbox so
    /// `handle_message` is not blocked for the minutes a sweep takes.
    fn begin_and_spawn_batch(&self, channels: Vec<ChannelKey>, concurrency: usize) -> Result<()> {
        self.engine.begin_calibration_batch(channels, concurrency)?;
        let engine = Rc::clone(&self.engine);
        crate::rt::spawn(async move {
            engine.drive_calibration_batch().await;
        });
        Ok(())
    }
}

impl ApiActor<CalibrationMessage> for CalibrationActor {
    fn name(&self) -> &'static str {
        "CalibrationActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<CalibrationMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: CalibrationMessage) {
        match msg {
            CalibrationMessage::Start {
                device_uid,
                channel_name,
                respond_to,
            } => {
                let _ = respond_to.send(self.spawn_diagnosis(device_uid, channel_name));
            }
            CalibrationMessage::Cancel {
                device_uid,
                channel_name,
                respond_to,
            } => {
                let key: ChannelKey = (device_uid, channel_name);
                let cancelled = self.engine.cancel_calibration_diagnosis(&key);
                let _ = respond_to.send(cancelled);
            }
            CalibrationMessage::Get {
                device_uid,
                channel_name,
                respond_to,
            } => {
                let key: ChannelKey = (device_uid, channel_name);
                let calibration = self.engine.calibration_store_get(&key);
                let _ = respond_to.send(calibration);
            }
            CalibrationMessage::GetAll { respond_to } => {
                let entries = self.engine.calibration_store_all();
                let _ = respond_to.send(entries);
            }
            CalibrationMessage::Delete {
                device_uid,
                channel_name,
                respond_to,
            } => {
                let key: ChannelKey = (device_uid, channel_name);
                let result = self.engine.delete_calibration(&key).await;
                let _ = respond_to.send(result);
            }
            CalibrationMessage::SetOverrides {
                device_uid,
                channel_name,
                kick_boost_override,
                kick_duration_override_ms,
                walk_after_kick_override,
                respond_to,
            } => {
                let key: ChannelKey = (device_uid, channel_name);
                let result = self
                    .engine
                    .set_calibration_overrides(
                        &key,
                        kick_boost_override,
                        kick_duration_override_ms,
                        walk_after_kick_override,
                    )
                    .await;
                let _ = respond_to.send(result);
            }
            CalibrationMessage::InProgress {
                device_uid,
                channel_name,
                respond_to,
            } => {
                let key: ChannelKey = (device_uid, channel_name);
                let _ = respond_to.send(self.engine.is_calibration_in_progress(&key));
            }
            CalibrationMessage::Status {
                device_uid,
                channel_name,
                respond_to,
            } => {
                let key: ChannelKey = (device_uid, channel_name);
                let _ = respond_to.send(self.engine.calibration_status(&key));
            }
            CalibrationMessage::StartBatch {
                channels,
                concurrency,
                respond_to,
            } => {
                let _ = respond_to.send(self.begin_and_spawn_batch(channels, concurrency));
            }
            CalibrationMessage::CancelBatch { respond_to } => {
                let _ = respond_to.send(self.engine.cancel_calibration_batch());
            }
            CalibrationMessage::BatchStatus { respond_to } => {
                let _ = respond_to.send(self.engine.calibration_batch_status());
            }
        }
    }
}

/// External handle to the calibration actor.
#[derive(Clone)]
pub struct CalibrationHandle {
    sender: mpsc::Sender<CalibrationMessage>,
}

impl CalibrationHandle {
    pub fn new<'s>(
        engine: Rc<Engine>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(CALIBRATION_ACTOR_QUEUE_DEPTH);
        let handle = Self {
            sender: sender.clone(),
        };
        let actor = CalibrationActor::new(receiver, engine);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        handle
    }

    pub async fn start(&self, device_uid: DeviceUID, channel_name: ChannelName) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::Start {
                device_uid,
                channel_name,
                respond_to: tx,
            })
            .await;
        rx.await?
    }

    /// Returns `true` if a sweep was actively in flight and was
    /// cancelled. Returns `false` if no sweep was running or if the
    /// actor task is gone (transport failure): both render the same
    /// "nothing to cancel" outcome to the caller.
    pub async fn cancel(&self, device_uid: DeviceUID, channel_name: ChannelName) -> bool {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::Cancel {
                device_uid,
                channel_name,
                respond_to: tx,
            })
            .await;
        rx.await.unwrap_or(false)
    }

    pub async fn get(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
    ) -> Option<Calibration> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::Get {
                device_uid,
                channel_name,
                respond_to: tx,
            })
            .await;
        rx.await.ok().flatten()
    }

    /// Snapshot every persisted calibration. Used by the bulk
    /// `GET /calibrations` route so the UI can mark calibrated channels
    /// in the tree at app load without a request per channel. Returns
    /// an empty `Vec` on transport failure rather than `Option` so
    /// callers don't have to distinguish "no calibrations" from "actor
    /// is gone" -- both render the same in the UI.
    pub async fn get_all(&self) -> Vec<CalibrationEntry> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::GetAll { respond_to: tx })
            .await;
        rx.await.unwrap_or_default()
    }

    pub async fn delete(&self, device_uid: DeviceUID, channel_name: ChannelName) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::Delete {
                device_uid,
                channel_name,
                respond_to: tx,
            })
            .await;
        rx.await?
    }

    /// Replace the override fields on the stored calibration. `Ok(None)`
    /// when no calibration exists for the channel (handler maps to 404).
    pub async fn set_overrides(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        kick_boost_override: Option<bool>,
        kick_duration_override_ms: Option<u32>,
        walk_after_kick_override: Option<bool>,
    ) -> Result<Option<Calibration>> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::SetOverrides {
                device_uid,
                channel_name,
                kick_boost_override,
                kick_duration_override_ms,
                walk_after_kick_override,
                respond_to: tx,
            })
            .await;
        rx.await?
    }

    /// Returns `false` on transport failure (actor task gone) as well
    /// as the literal "no sweep is in flight" answer. Callers should
    /// not distinguish the two: a missing actor will surface elsewhere
    /// as a request failure, so this returning `false` is safe.
    pub async fn in_progress(&self, device_uid: DeviceUID, channel_name: ChannelName) -> bool {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::InProgress {
                device_uid,
                channel_name,
                respond_to: tx,
            })
            .await;
        rx.await.unwrap_or(false)
    }

    /// Fetch the current per-channel status. Returns `None` only on
    /// a transport-level failure (actor task dropped the channel);
    /// otherwise the status itself encodes the "nothing yet" state
    /// as `NotStarted` and never round-trips an absent value.
    pub async fn status(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
    ) -> Option<CalibrationStatus> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::Status {
                device_uid,
                channel_name,
                respond_to: tx,
            })
            .await;
        rx.await.ok()
    }

    /// Begin a calibration batch, `concurrency` sweeps at a time (1 =
    /// sequential). `Err` when one is already active or the request is
    /// invalid (the handler maps it to 409).
    pub async fn start_batch(&self, channels: Vec<ChannelKey>, concurrency: usize) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::StartBatch {
                channels,
                concurrency,
                respond_to: tx,
            })
            .await;
        rx.await?
    }

    /// Cancel the active batch. `false` when none is active (or the
    /// actor task is gone): both render as "nothing to cancel".
    pub async fn cancel_batch(&self) -> bool {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::CancelBatch { respond_to: tx })
            .await;
        rx.await.unwrap_or(false)
    }

    /// Snapshot the active or most recent batch. `None` when no batch
    /// has run this session, or on a transport failure.
    pub async fn batch_status(&self) -> Option<CalibrationBatchStatus> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(CalibrationMessage::BatchStatus { respond_to: tx })
            .await;
        rx.await.ok().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_fields(status: &CalibrationStatus) -> (&str, &str) {
        match status {
            CalibrationStatus::Failed {
                reason, message, ..
            } => (reason.as_str(), message.as_str()),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn preflight_temp_failure_message_names_the_offending_sensor() {
        // Goal: a pre-flight temp block carries the offending sensor's
        // identity into the UI message, not just the number, so the
        // user sees which reading (often an unrelated device) blocked
        // the sweep. The reason code stays stable for the UI to key on.
        let failure = DiagnosisFailure::PreflightTempTooHigh {
            observed: 86.0,
            limit: 75.0,
            sensor: "CPU | Tctl".to_string(),
        };
        let status =
            CalibrationStatus::from_failure("dev-a".to_string(), "fan1".to_string(), &failure);
        let (reason, message) = failed_fields(&status);
        assert_eq!(reason, "preflight_temp_too_high");
        assert!(message.contains("CPU | Tctl"), "message: {message}");
        assert!(message.contains("86.0"), "message: {message}");
        assert!(message.contains("75.0"), "message: {message}");
    }

    #[test]
    fn temp_abort_message_names_the_offending_sensor() {
        // Goal: same guarantee for a mid-sweep abort.
        let failure = DiagnosisFailure::TempAbortedAt {
            observed: 90.0,
            limit: 85.0,
            sensor: "GPU | edge".to_string(),
        };
        let status =
            CalibrationStatus::from_failure("dev-a".to_string(), "fan1".to_string(), &failure);
        let (reason, message) = failed_fields(&status);
        assert_eq!(reason, "temp_aborted");
        assert!(message.contains("GPU | edge"), "message: {message}");
        assert!(message.contains("90.0"), "message: {message}");
        assert!(message.contains("85.0"), "message: {message}");
    }
}
