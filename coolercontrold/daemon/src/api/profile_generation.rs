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
//! logic is built up across phases: currently the CPU air cooler is implemented end to end,
//! and the remaining kinds are skipped until their phase.

use crate::api::{AppState, CCError};
use crate::device::{ChannelName, DeviceUID, Duty, Temp};
use crate::setting::{
    CustomSensor, Function, FunctionUID, Profile, ProfileKind, ProfileUID, TempSource,
};
use axum::extract::State;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Not;
use strum::{Display, EnumString};
use uuid::Uuid;

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

impl GenerateProfilesRequest {
    /// The preset that applies to a kind: its per-kind override if present, else the global
    /// preset.
    fn effective_preset(&self, kind: FanKind) -> Preset {
        self.preset_overrides
            .iter()
            .find(|override_entry| override_entry.kind == kind)
            .map_or(self.global_preset, |override_entry| override_entry.preset)
    }
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

/// Proposes profiles, functions, and custom sensors for the assigned fans, without
/// persisting anything. The UI previews the result and the user confirms before it is saved.
pub async fn generate(
    State(AppState { .. }): State<AppState>,
    Json(request): Json<GenerateProfilesRequest>,
) -> Result<Json<GenerateProfilesResponse>, CCError> {
    generate_proposal(&request).map(Json)
}

/// Builds the proposed entity set for a request. Pure and synchronous: it depends only on the
/// request, so it is fully unit-testable without the daemon's state.
fn generate_proposal(
    request: &GenerateProfilesRequest,
) -> Result<GenerateProfilesResponse, CCError> {
    let mut proposal = Proposal::with_capacity(request.assignments.len());
    for assignment in &request.assignments {
        let preset = request.effective_preset(assignment.kind);
        add_assignment(&mut proposal, &request.key_temps, assignment, preset)?;
    }
    Ok(proposal.into_response())
}

/// Dispatches one fan assignment to its kind-specific generator. The match is exhaustive so
/// that adding a kind later is a compile error until it is handled here. Kinds beyond the CPU
/// cooler are filled in their later phase and currently contribute nothing.
fn add_assignment(
    proposal: &mut Proposal,
    key_temps: &KeyTemps,
    assignment: &FanAssignment,
    preset: Preset,
) -> Result<(), CCError> {
    match assignment.kind {
        FanKind::CpuCooler => {
            let cpu_temp = key_temps.cpu.as_ref().ok_or_else(|| CCError::UserError {
                msg: "A CPU air cooler was assigned but no CPU temp was selected".to_string(),
            })?;
            add_cpu_cooler(proposal, assignment, cpu_temp, preset);
        }
        FanKind::GpuFan
        | FanKind::AioRadiator
        | FanKind::AioPump
        | FanKind::CaseIntake
        | FanKind::CaseExhaust
        | FanKind::LaptopFan => {}
    }
    Ok(())
}

/// Generates a CPU-air-cooler profile (Graph off the CPU temp) plus its hysteresis function,
/// and assigns the fan to it. The curve here is a simple placeholder ramp; the expert,
/// preset-specific curves and Silent EMA smoothing land in a later phase.
fn add_cpu_cooler(
    proposal: &mut Proposal,
    assignment: &FanAssignment,
    cpu_temp: &TempSource,
    preset: Preset,
) {
    let function_uid = proposal.intern_function(build_downward_function(&format!("Auto {preset}")));
    let speed_profile = placeholder_cpu_curve();
    debug_assert!(speed_profile.is_empty().not(), "curve must have points");
    debug_assert!(
        speed_profile.windows(2).all(|w| w[0].0 < w[1].0),
        "curve temps must strictly increase"
    );
    debug_assert!(
        speed_profile.windows(2).all(|w| w[0].1 <= w[1].1),
        "curve duties must not decrease"
    );
    let profile = build_graph_profile(
        &format!("CPU Cooler ({preset})"),
        cpu_temp.clone(),
        function_uid,
        speed_profile,
    );
    let profile_uid = proposal.intern_profile(profile);
    proposal.assignments.push(ChannelAssignment {
        device_uid: assignment.device_uid.clone(),
        channel_name: assignment.channel_name.clone(),
        profile_uid,
        replaces_profile_name: None,
    });
}

/// A simple monotonic CPU ramp used until the expert per-preset curves arrive.
fn placeholder_cpu_curve() -> Vec<(Temp, Duty)> {
    vec![(30.0, 30), (50.0, 50), (70.0, 80), (85.0, 100)]
}

/// A Graph profile following a single temp source. Members, mix, offset, and fixed speed are
/// left unset; a fresh UID is assigned.
fn build_graph_profile(
    name: &str,
    temp_source: TempSource,
    function_uid: FunctionUID,
    speed_profile: Vec<(Temp, Duty)>,
) -> Profile {
    Profile {
        uid: Uuid::new_v4().to_string(),
        name: name.to_string(),
        function_uid,
        kind: ProfileKind::Graph {
            speed_profile: Some(speed_profile),
            temp_source: Some(temp_source),
            temp_min: None,
            temp_max: None,
        },
    }
}

/// A downward-only hysteresis function, used by the Balanced and Performance presets so fans
/// ramp up promptly but settle down gently. A fresh UID is assigned.
fn build_downward_function(name: &str) -> Function {
    Function {
        uid: Uuid::new_v4().to_string(),
        name: name.to_string(),
        only_downward: Some(true),
        ..Function::default()
    }
}

/// Accumulates the entities a generation run proposes, de-duplicating functions and profiles
/// that share an identical definition so the user's lists are not cluttered with copies.
struct Proposal {
    custom_sensors: Vec<CustomSensor>,
    functions: Vec<Function>,
    profiles: Vec<Profile>,
    assignments: Vec<ChannelAssignment>,
    function_uid_by_signature: HashMap<String, FunctionUID>,
    profile_uid_by_signature: HashMap<String, ProfileUID>,
}

impl Proposal {
    fn with_capacity(fan_count: usize) -> Self {
        Self {
            custom_sensors: Vec::new(),
            functions: Vec::with_capacity(fan_count),
            profiles: Vec::with_capacity(fan_count),
            assignments: Vec::with_capacity(fan_count),
            function_uid_by_signature: HashMap::with_capacity(fan_count),
            profile_uid_by_signature: HashMap::with_capacity(fan_count),
        }
    }

    /// Returns the UID of an already-proposed identical function, or stores this one and
    /// returns its UID. The passed function's fresh UID is discarded on a dedup hit.
    fn intern_function(&mut self, function: Function) -> FunctionUID {
        let signature = function_signature(&function);
        if let Some(existing_uid) = self.function_uid_by_signature.get(&signature) {
            return existing_uid.clone();
        }
        let uid = function.uid.clone();
        self.function_uid_by_signature
            .insert(signature, uid.clone());
        self.functions.push(function);
        uid
    }

    /// Returns the UID of an already-proposed identical profile, or stores this one and
    /// returns its UID. The passed profile's fresh UID is discarded on a dedup hit.
    fn intern_profile(&mut self, profile: Profile) -> ProfileUID {
        let signature = profile_signature(&profile);
        if let Some(existing_uid) = self.profile_uid_by_signature.get(&signature) {
            return existing_uid.clone();
        }
        let uid = profile.uid.clone();
        self.profile_uid_by_signature.insert(signature, uid.clone());
        self.profiles.push(profile);
        uid
    }

    fn into_response(self) -> GenerateProfilesResponse {
        GenerateProfilesResponse {
            custom_sensors: self.custom_sensors,
            functions: self.functions,
            profiles: self.profiles,
            assignments: self.assignments,
        }
    }
}

/// A definition fingerprint of a function, excluding its UID and name, so two functions that
/// only differ by those are treated as duplicates. Debug formatting is a compact, stable
/// stand-in for structural equality (the types do not derive `PartialEq`).
fn function_signature(function: &Function) -> String {
    format!(
        "{:?}|{}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{}|{}",
        function.f_type,
        function.step_size_min,
        function.step_size_max,
        function.step_size_min_decreasing,
        function.step_size_max_decreasing,
        function.response_delay,
        function.deviance,
        function.only_downward,
        function.sample_window,
        function.threshold_hopping,
        function.bypass_min_at_extremes,
    )
}

/// A definition fingerprint of a profile, excluding its UID and name. Includes the (already
/// de-duplicated) `function_uid` plus the `kind` (which carries the type and all type-specific
/// fields), so profiles sharing a function and curve collapse together.
fn profile_signature(profile: &Profile) -> String {
    format!("{}|{:?}", profile.function_uid, profile.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::{ProfileType, TempSource};

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

    fn cpu_temp() -> TempSource {
        TempSource {
            temp_name: "Tctl".to_string(),
            device_uid: "dev-cpu-1".to_string(),
        }
    }

    fn cpu_cooler_request(channel_names: &[&str], preset: Preset) -> GenerateProfilesRequest {
        let assignments = channel_names
            .iter()
            .map(|channel_name| FanAssignment {
                device_uid: "dev-mb-1".to_string(),
                channel_name: (*channel_name).to_string(),
                kind: FanKind::CpuCooler,
                position: None,
                laptop_temp_strategy: None,
            })
            .collect();
        GenerateProfilesRequest {
            assignments,
            key_temps: KeyTemps {
                cpu: Some(cpu_temp()),
                gpu: None,
                liquid: None,
                ambient: None,
            },
            global_preset: preset,
            preset_overrides: Vec::new(),
        }
    }

    /// Goal: a single CPU cooler yields a valid Graph profile plus its function, wired
    /// together and assigned to the fan. Method: generate, then assert the entity counts, the
    /// profile shape, and that the assignment points at the produced profile.
    #[test]
    fn generates_cpu_cooler_profile_and_function() {
        let response =
            generate_proposal(&cpu_cooler_request(&["fan1"], Preset::Balanced)).expect("generates");
        assert_eq!(response.profiles.len(), 1);
        assert_eq!(response.functions.len(), 1);
        assert_eq!(response.assignments.len(), 1);

        let profile = &response.profiles[0];
        assert_eq!(profile.p_type(), ProfileType::Graph);
        assert_eq!(profile.temp_source(), Some(&cpu_temp()));
        assert!(profile.speed_profile().is_some_and(|c| c.is_empty().not()));

        let function = &response.functions[0];
        assert_eq!(function.only_downward, Some(true));
        assert_eq!(profile.function_uid, function.uid);
        assert_eq!(response.assignments[0].profile_uid, profile.uid);
        assert!(response.assignments[0].replaces_profile_name.is_none());
    }

    /// Goal: two CPU coolers with the same preset and temp source collapse to one shared
    /// profile and function, with both fans assigned to it. Method: generate for two channels
    /// and assert the dedup leaves one profile/function but two assignments to the same UID.
    #[test]
    fn dedups_identical_cpu_coolers() {
        let response = generate_proposal(&cpu_cooler_request(&["fan1", "fan2"], Preset::Balanced))
            .expect("generates");
        assert_eq!(response.profiles.len(), 1, "identical profiles share one");
        assert_eq!(response.functions.len(), 1, "identical functions share one");
        assert_eq!(
            response.assignments.len(),
            2,
            "each fan still gets assigned"
        );
        assert_eq!(
            response.assignments[0].profile_uid,
            response.assignments[1].profile_uid
        );
    }

    /// Goal: assigning a CPU cooler without a CPU temp is a user error caught at the boundary.
    /// Method: omit the CPU temp and assert generation returns an error.
    #[test]
    fn cpu_cooler_without_cpu_temp_errors() {
        let mut request = cpu_cooler_request(&["fan1"], Preset::Balanced);
        request.key_temps.cpu = None;
        assert!(generate_proposal(&request).is_err());
    }

    /// Goal: per-kind overrides win over the global preset, and unlisted kinds fall back to
    /// global. Method: set a `CpuCooler` override and assert the resolved presets.
    #[test]
    fn effective_preset_uses_override_then_global() {
        let request = sample_request();
        assert_eq!(request.global_preset, Preset::Balanced);
        assert_eq!(
            request.effective_preset(FanKind::CpuCooler),
            Preset::Performance
        );
        assert_eq!(request.effective_preset(FanKind::GpuFan), Preset::Balanced);
    }

    /// Goal: a request with no assignments proposes nothing. Method: generate an empty request
    /// and assert every collection is empty.
    #[test]
    fn empty_assignments_yield_empty_response() {
        let request = GenerateProfilesRequest {
            assignments: Vec::new(),
            key_temps: KeyTemps {
                cpu: None,
                gpu: None,
                liquid: None,
                ambient: None,
            },
            global_preset: Preset::Silent,
            preset_overrides: Vec::new(),
        };
        let response = generate_proposal(&request).expect("generates");
        assert!(response.profiles.is_empty());
        assert!(response.functions.is_empty());
        assert!(response.custom_sensors.is_empty());
        assert!(response.assignments.is_empty());
    }
}
