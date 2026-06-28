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
    run_diagnosis, DiagnosisFailure, DiagnosisHost, DiagnosisPhase, DiagnosisProgress,
    DiagnosisSettings, SettingsSnapshot, SnapshotKind,
};
pub use dispatch::{dispatch, DutyWriter, RepoWriter};
pub use registry::DiagnosisRegistry;
pub use state::FanStateMap;
pub use store::{CalibrationEntry, CalibrationStore};

use crate::device::{ChannelName, DeviceUID};

/// Identifies a calibratable channel uniquely within the daemon.
pub type ChannelKey = (DeviceUID, ChannelName);
