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

//! Per-channel RPM calibration.
//!
//! User-triggered diagnosis sweeps a fan's duty range and persists a
//! `Calibration` curve. The runtime then maps user-facing **true-duty**
//! (RPM-normalized: 0% = no spin, 100% = max effective RPM) to the
//! device-duty actually written to hardware, and inversely maps measured
//! RPM back to true-duty for status display.
//!
//! Phase 1 ships the data types, mapping math, step-curve detection, and
//! JSON persistence. The dispatch state machine, the diagnoser sweep, and
//! the REST/SSE API land in later phases, which is why several public
//! items here are dead until those phases consume them.

// Phase 1 staging: consumers of these public APIs (dispatch state machine,
// diagnoser, REST/SSE handlers) land in subsequent phases. The allow is
// removed as soon as the first consumer is added.
#![allow(dead_code, unused_imports)]

mod curve;
mod diagnoser;
mod dispatch;
mod registry;
mod state;
mod store;

pub use curve::{
    classify_curve, derive_scalars, start_threshold, Calibration, CurveKind, DerivedScalars,
    MappedDuty, DUTY_STEP_PERCENT, SAMPLE_COUNT,
};
pub use diagnoser::{
    run_diagnosis, DiagnosisFailure, DiagnosisHost, DiagnosisPhase, DiagnosisProgress,
    DiagnosisSettings, SettingsSnapshot, SnapshotKind,
};
pub use dispatch::{
    complete_kick, dispatch, dispatch_local, DispatchOutcome, DutyWriter, RepoWriter,
};
pub use registry::DiagnosisRegistry;
pub use state::{ChannelEntry, FanState, FanStateMap};
pub use store::{CalibrationConfigFile, CalibrationEntry, CalibrationStore};

use crate::device::{ChannelName, DeviceUID};

/// Identifies a calibratable channel uniquely within the daemon.
pub type ChannelKey = (DeviceUID, ChannelName);
