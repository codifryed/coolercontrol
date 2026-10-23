// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::{handle_error, AppState, CCError};
use crate::device::UID;
use crate::listeners::power_profile::PowerProfileSnapshot;
use axum::extract::State;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ops::Not;

/// Upper bound on mapped profiles. `power-profiles-daemon` offers three; the cap only exists so
/// a hand-crafted request cannot grow the config file without limit.
const MAX_MAPPED_PROFILES: usize = 32;
const MAX_PROFILE_NAME_LENGTH: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PowerProfileModesDto {
    /// Power profile name to Mode UID. Profiles left out are unmapped and are ignored when the
    /// system switches to them.
    pub modes: HashMap<String, UID>,
}

/// The available system power profiles, the active one, and the profile to Mode mapping.
///
/// `available` is empty when no power profile daemon is reachable, which is how a client decides
/// whether to offer the feature at all.
pub async fn get(
    State(AppState { power_profiles, .. }): State<AppState>,
) -> Json<PowerProfileSnapshot> {
    Json(power_profiles.snapshot())
}

/// Replaces the power profile to Mode mapping.
///
/// Every referenced Mode is verified to exist first, and `ModeController::delete_mode` prunes
/// any mapping that pointed at a deleted Mode, so a mapping can never point at a Mode that is
/// gone and silently do nothing when the profile changes.
pub async fn update_modes(
    State(AppState {
        power_profiles,
        mode_handle,
        setting_handle,
        ..
    }): State<AppState>,
    Json(dto): Json<PowerProfileModesDto>,
) -> Result<(), CCError> {
    validate_shape(&dto.modes)?;
    let known: HashSet<UID> = mode_handle
        .get_all()
        .await
        .map_err(handle_error)?
        .into_iter()
        .map(|mode| mode.uid)
        .collect();
    for (profile, mode_uid) in &dto.modes {
        if known.contains(mode_uid).not() {
            return Err(CCError::NotFound {
                msg: format!("Mode {mode_uid} mapped to power profile '{profile}' not found"),
            });
        }
    }
    setting_handle
        .set_power_profile_modes(dto.modes.clone())
        .await
        .map_err(handle_error)?;
    // Only after the write succeeds, so a failed save cannot leave the listener acting on a
    // mapping that was never persisted.
    power_profiles.set_modes(dto.modes);
    Ok(())
}

/// Rejects a mapping that is malformed regardless of which Modes exist.
fn validate_shape(modes: &HashMap<String, UID>) -> Result<(), CCError> {
    if modes.len() > MAX_MAPPED_PROFILES {
        return Err(CCError::UserError {
            msg: format!("At most {MAX_MAPPED_PROFILES} power profiles can be mapped."),
        });
    }
    for (profile, mode_uid) in modes {
        if profile.trim().is_empty() {
            return Err(CCError::UserError {
                msg: "A power profile name cannot be empty.".to_string(),
            });
        }
        if profile.len() > MAX_PROFILE_NAME_LENGTH {
            return Err(CCError::UserError {
                msg: format!(
                    "Power profile names cannot be longer than {MAX_PROFILE_NAME_LENGTH} \
                     characters."
                ),
            });
        }
        if mode_uid.trim().is_empty() {
            return Err(CCError::UserError {
                msg: format!("The Mode UID for power profile '{profile}' cannot be empty."),
            });
        }
    }
    debug_assert!(modes.len() <= MAX_MAPPED_PROFILES);
    debug_assert!(modes.keys().all(|profile| profile.trim().is_empty().not()));
    debug_assert!(modes
        .values()
        .all(|mode_uid| mode_uid.trim().is_empty().not()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: a normal three-profile mapping is accepted, since that is what the UI sends.
    /// Methodology: validate the shape ppd actually offers.
    #[test]
    fn a_normal_mapping_passes_validation() {
        let modes = HashMap::from([
            ("power-saver".to_string(), "uid-1".to_string()),
            ("balanced".to_string(), "uid-2".to_string()),
            ("performance".to_string(), "uid-3".to_string()),
        ]);

        assert!(validate_shape(&modes).is_ok());
    }

    /// Goal: an empty mapping is how a client clears the feature and must be accepted.
    /// Methodology: validate an empty map.
    #[test]
    fn an_empty_mapping_passes_validation() {
        assert!(validate_shape(&HashMap::new()).is_ok());
    }

    /// Goal: blank names and blank UIDs must be rejected at the boundary rather than written to
    /// the config, where they would be silently skipped on the next read.
    /// Methodology: submit each blank form on its own.
    #[test]
    fn blank_names_and_uids_are_rejected() {
        let blank_profile = HashMap::from([("   ".to_string(), "uid-1".to_string())]);
        assert!(
            validate_shape(&blank_profile).is_err(),
            "blank profile name"
        );

        let blank_uid = HashMap::from([("balanced".to_string(), String::new())]);
        assert!(validate_shape(&blank_uid).is_err(), "blank mode uid");
    }

    /// Goal: the mapping is bounded, so a crafted request cannot grow the config without limit.
    /// Methodology: submit one entry more than the cap, and confirm the cap itself is allowed.
    #[test]
    fn an_oversized_mapping_is_rejected() {
        let at_cap: HashMap<String, UID> = (0..MAX_MAPPED_PROFILES)
            .map(|i| (format!("profile-{i}"), format!("uid-{i}")))
            .collect();
        assert!(validate_shape(&at_cap).is_ok(), "the cap itself is allowed");

        let over_cap: HashMap<String, UID> = (0..=MAX_MAPPED_PROFILES)
            .map(|i| (format!("profile-{i}"), format!("uid-{i}")))
            .collect();
        assert!(validate_shape(&over_cap).is_err());
    }

    /// Goal: an over-long profile name is rejected, since it only ever comes from a hand-crafted
    /// request and would bloat the config file.
    /// Methodology: submit a name one character past the limit.
    #[test]
    fn an_overlong_profile_name_is_rejected() {
        let long = "p".repeat(MAX_PROFILE_NAME_LENGTH + 1);
        let modes = HashMap::from([(long, "uid-1".to_string())]);

        assert!(validate_shape(&modes).is_err());
    }
}
