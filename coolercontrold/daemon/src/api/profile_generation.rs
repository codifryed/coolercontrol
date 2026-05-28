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

//! The wire contract for the auto-create-profiles wizard.
//!
//! The wizard sends the user's fan-role assignments, chosen key temps, and presets; the
//! daemon returns a proposed set of profiles, functions, and custom sensors plus the
//! channel assignments to apply. Nothing here is persisted: the UI previews the proposal,
//! the user confirms, and the existing create endpoints persist it. The per-kind generation
//! logic that fills the response lands in a later phase; this module currently defines the
//! contract and a stub endpoint that returns an empty proposal.

use crate::api::{AppState, CCError};
use crate::device::{ChannelName, DeviceUID};
use crate::setting::{CustomSensor, Function, Profile, ProfileUID, TempSource};
use axum::extract::State;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// The cooling role a fan plays. Assigned explicitly by the user: fan roles cannot be
/// reliably auto-detected (an AIO pump can be wired to an ordinary motherboard fan header),
/// so a wrong guess is worse than none.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize, JsonSchema,
)]
pub enum FanKind {
    CpuCooler,
    GpuFan,
    AioRadiator,
    AioPump,
    CaseIntake,
    CaseExhaust,
    LaptopFan,
}

/// The noise/performance tradeoff applied to a generated profile.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize, JsonSchema,
)]
pub enum Preset {
    Silent,
    Balanced,
    Performance,
}

/// A case-fan mounting position. Label only: it affects generated profile names, never the
/// generated curve.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize, JsonSchema,
)]
pub enum FanPosition {
    Top,
    Front,
    Back,
    Bottom,
}

/// Which temperature a laptop fan should follow. Honored only when the fan's kind is
/// `LaptopFan`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize, JsonSchema,
)]
pub enum LaptopTempStrategy {
    /// An EMA custom sensor wrapping the real CPU temp. Sensible default for most laptops.
    EmaCpu,
    /// The `ThinkPad` CPU temp sensor read directly.
    ThinkpadSensor,
    /// A Mix(CPU, GPU) using the Max function, so a disabled dGPU reading 0C is ignored.
    MixCpuGpu,
}

/// One fan the user has assigned a cooling role to. Fans the user skips are omitted from
/// the request entirely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FanAssignment {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub kind: FanKind,

    /// Case-fan mounting position, used only to name the generated profile.
    pub position: Option<FanPosition>,

    /// Laptop temp strategy, honored only when `kind` is `LaptopFan`.
    pub laptop_temp_strategy: Option<LaptopTempStrategy>,
}

/// The canonical system temps the user has identified, pre-filled by the UI but verified by
/// the user. Each is optional because not every system exposes all of them (no dGPU,
/// air-cooled, no ambient probe). A CPU temp is the minimum needed to generate anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KeyTemps {
    pub cpu: Option<TempSource>,
    pub gpu: Option<TempSource>,
    pub liquid: Option<TempSource>,
    pub ambient: Option<TempSource>,
}

/// A per-kind preset that overrides the global preset for one kind. Case intake and exhaust
/// are coupled (they share one base Mix profile), so both must carry the same preset; that
/// is enforced at the generation boundary, not by the type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresetOverride {
    pub kind: FanKind,
    pub preset: Preset,
}

/// The full input to one profile-generation run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GenerateProfilesRequest {
    pub assignments: Vec<FanAssignment>,
    pub key_temps: KeyTemps,
    pub global_preset: Preset,

    #[serde(default)]
    pub preset_overrides: Vec<PresetOverride>,
}

/// A fan-to-profile assignment the run proposes. `replaces_profile_name` is set when the
/// channel already has a non-default profile that Create & Apply would replace, so the UI
/// can warn the user before overwriting it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelAssignment {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub profile_uid: ProfileUID,
    pub replaces_profile_name: Option<String>,
}

/// The proposed result of a generation run. Nothing here is persisted; the UI previews it
/// and the user confirms before the existing create endpoints persist it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GenerateProfilesResponse {
    pub custom_sensors: Vec<CustomSensor>,
    pub functions: Vec<Function>,
    pub profiles: Vec<Profile>,
    pub assignments: Vec<ChannelAssignment>,
}

impl GenerateProfilesResponse {
    /// An empty proposal: no entities and no assignments.
    fn empty() -> Self {
        Self {
            custom_sensors: Vec::new(),
            functions: Vec::new(),
            profiles: Vec::new(),
            assignments: Vec::new(),
        }
    }
}

/// Proposes profiles, functions, and custom sensors for the assigned fans, without
/// persisting anything.
///
/// Phase 0 stub: the per-kind generation logic is implemented in a later phase, so this
/// returns an empty proposal. It exists now so the API contract is callable and testable.
pub async fn generate(
    State(AppState { .. }): State<AppState>,
    Json(_request): Json<GenerateProfilesRequest>,
) -> Result<Json<GenerateProfilesResponse>, CCError> {
    Ok(Json(GenerateProfilesResponse::empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::TempSource;

    fn sample_request() -> GenerateProfilesRequest {
        GenerateProfilesRequest {
            assignments: vec![
                FanAssignment {
                    device_uid: "dev-hwmon-1".to_string(),
                    channel_name: "fan2".to_string(),
                    kind: FanKind::CaseIntake,
                    position: Some(FanPosition::Front),
                    laptop_temp_strategy: None,
                },
                FanAssignment {
                    device_uid: "dev-laptop-1".to_string(),
                    channel_name: "fan1".to_string(),
                    kind: FanKind::LaptopFan,
                    position: None,
                    laptop_temp_strategy: Some(LaptopTempStrategy::EmaCpu),
                },
            ],
            key_temps: KeyTemps {
                cpu: Some(TempSource {
                    temp_name: "Tctl".to_string(),
                    device_uid: "dev-cpu-1".to_string(),
                }),
                gpu: None,
                liquid: None,
                ambient: None,
            },
            global_preset: Preset::Balanced,
            preset_overrides: vec![PresetOverride {
                kind: FanKind::CpuCooler,
                preset: Preset::Performance,
            }],
        }
    }

    /// Goal: the request type survives a JSON round trip unchanged, so the UI and daemon
    /// agree on the contract. Method: serialize a representative request, deserialize it
    /// back, and assert equality.
    #[test]
    fn request_round_trips_through_json() {
        let request = sample_request();
        let json = serde_json::to_string(&request).expect("request serializes");
        let parsed: GenerateProfilesRequest =
            serde_json::from_str(&json).expect("request deserializes");
        assert_eq!(request, parsed);
    }

    /// Goal: `preset_overrides` is optional on the wire so the UI may omit it. Method: parse
    /// a request JSON that has no `preset_overrides` field and assert it defaults to empty.
    #[test]
    fn request_preset_overrides_default_to_empty() {
        let json = r#"{
            "assignments": [],
            "key_temps": {"cpu": null, "gpu": null, "liquid": null, "ambient": null},
            "global_preset": "Silent"
        }"#;
        let parsed: GenerateProfilesRequest =
            serde_json::from_str(json).expect("request without overrides deserializes");
        assert!(parsed.preset_overrides.is_empty());
        assert_eq!(parsed.global_preset, Preset::Silent);
    }

    /// Goal: the enum wire strings are stable, because the UI sends and matches these exact
    /// values. Method: serialize each enum and assert the `PascalCase` tokens.
    #[test]
    fn enum_wire_strings_are_stable() {
        assert_eq!(
            serde_json::to_string(&FanKind::AioPump).unwrap(),
            "\"AioPump\""
        );
        assert_eq!(
            serde_json::to_string(&FanKind::CaseExhaust).unwrap(),
            "\"CaseExhaust\""
        );
        assert_eq!(
            serde_json::to_string(&Preset::Performance).unwrap(),
            "\"Performance\""
        );
        assert_eq!(
            serde_json::to_string(&LaptopTempStrategy::MixCpuGpu).unwrap(),
            "\"MixCpuGpu\""
        );
        assert_eq!(
            serde_json::to_string(&FanPosition::Bottom).unwrap(),
            "\"Bottom\""
        );
    }

    /// Goal: the empty proposal serializes to empty arrays, the shape the UI expects before
    /// any generation logic exists. Method: serialize `empty()` to a JSON value and assert
    /// every collection is an empty array.
    #[test]
    fn empty_response_serializes_to_empty_arrays() {
        let value = serde_json::to_value(GenerateProfilesResponse::empty())
            .expect("empty response serializes");
        for key in ["custom_sensors", "functions", "profiles", "assignments"] {
            assert_eq!(value[key], serde_json::json!([]), "{key} should be empty");
        }
    }
}
