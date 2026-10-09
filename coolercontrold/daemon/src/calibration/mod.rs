// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-channel RPM calibration.
//!
//! User-triggered diagnosis sweeps a fan's duty range and persists a
//! `Calibration` curve. The runtime then maps user-facing **true-duty**
//! (RPM-normalized: 0% = no spin, 100% = max effective RPM) to the
//! device-duty actually written to hardware, and inversely maps measured
//! RPM back to true-duty for status display.

mod augmenter;
mod batch;
mod curve;
mod diagnoser;
mod dispatch;
mod registry;
mod state;
mod store;

pub use augmenter::{install_augmenter_on_all_devices, CalibrationStatusAugmenter};
pub use batch::{BatchBeginError, BatchEntry, BatchEntryPhase, CalibrationBatchState};
#[cfg_attr(not(test), allow(unused_imports))]
pub use curve::DutySample;
pub use curve::{effective_speed_options, Calibration, CalibrationWarning, CurveKind};
pub use diagnoser::{
    others_over_limit_note, run_diagnosis, DiagnosisFailure, DiagnosisHost, DiagnosisPhase,
    DiagnosisProgress, DiagnosisSettings, HottestTemp, SettingsSnapshot, SnapshotKind,
    CALIBRATION_TEMP_HINT,
};
pub use dispatch::{dispatch, DutyWriter, RepoWriter};
pub use registry::DiagnosisRegistry;
pub use state::FanStateMap;
// Parameter and return types of `FanStateMap`'s public entry/replace
// pair; only the engine tests name them today.
#[cfg_attr(not(test), allow(unused_imports))]
pub use state::{ChannelEntry, FanState};
pub use store::{validate, CalibrationEntry, CalibrationStore};

use crate::device::{ChannelName, DeviceUID};

/// Identifies a calibratable channel uniquely within the daemon.
pub type ChannelKey = (DeviceUID, ChannelName);

/// Preflight gate: an alert already Active on a channel blocks calibrating it.
/// Suppression only exists for alerts a sweep would cause; one that fired
/// beforehand means the fan itself is suspect. Implemented by the alert
/// controller; tests install a stub.
pub trait CalibrationAlertGate {
    /// The name of an enabled, unsilenced alert currently Active on the
    /// channel via a non-Temp source, if any.
    fn active_alert_for_channel(&self, device_uid: &str, channel_name: &str) -> Option<String>;
}
