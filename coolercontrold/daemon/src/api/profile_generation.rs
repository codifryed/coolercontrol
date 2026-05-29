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

use crate::api::devices::{apply_effective_speed_options, build_calibration_map, DeviceDto};
use crate::api::{AppState, CCError};
use crate::device::{ChannelName, DeviceType, DeviceUID, Duty, Temp, TempName};
use crate::setting::{
    CustomSensor, CustomSensorMixFunctionType, CustomTempSourceData, Function, FunctionUID, Offset,
    Profile, ProfileKind, ProfileMixFunctionType, ProfileUID, CustomSensorKind, TempSource,
    DEFAULT_FUNCTION_UID,
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

/// EMA smoothing window for the Silent preset, in seconds. Long enough to absorb brief temp
/// spikes so a quiet fan does not chase them.
const SILENT_EMA_WINDOW_SECONDS: u16 = 8;

/// Proposes profiles, functions, and custom sensors for the assigned fans, without persisting
/// anything. The UI previews the result and the user confirms before it is saved.
pub async fn generate(
    State(AppState {
        device_handle,
        calibration_handle,
        ..
    }): State<AppState>,
    Json(request): Json<GenerateProfilesRequest>,
) -> Result<Json<GenerateProfilesResponse>, CCError> {
    let mut devices = device_handle.devices_get().await?;
    let calibration_map = build_calibration_map(&calibration_handle).await;
    apply_effective_speed_options(&mut devices, &calibration_map);
    let context = DeviceContext::from_devices(&devices);
    generate_proposal(&request, &context).map(Json)
}

/// Builds the proposed entity set for a request. Pure and synchronous: given the request and a
/// device snapshot it is fully unit-testable without the daemon's live state.
fn generate_proposal(
    request: &GenerateProfilesRequest,
    context: &DeviceContext,
) -> Result<GenerateProfilesResponse, CCError> {
    validate_case_preset_coupling(request)?;
    let mut proposal = Proposal::with_capacity(request.assignments.len());
    for assignment in &request.assignments {
        let preset = request.effective_preset(assignment.kind);
        add_assignment(
            &mut proposal,
            context,
            &request.key_temps,
            assignment,
            preset,
        )?;
    }
    Ok(proposal.into_response())
}

/// Case intake and exhaust share one base profile, so they must use the same preset. With case
/// fans present, reject a request that overrides them to different presets.
fn validate_case_preset_coupling(request: &GenerateProfilesRequest) -> Result<(), CCError> {
    let has_case_fan = request
        .assignments
        .iter()
        .any(|assignment| matches!(assignment.kind, FanKind::CaseIntake | FanKind::CaseExhaust));
    if has_case_fan.not() {
        return Ok(());
    }
    if request.effective_preset(FanKind::CaseIntake)
        != request.effective_preset(FanKind::CaseExhaust)
    {
        return Err(CCError::UserError {
            msg: "Case intake and exhaust must use the same preset".to_string(),
        });
    }
    Ok(())
}

/// Dispatches one fan assignment to its kind-specific generator. The match is exhaustive so
/// that adding a kind later is a compile error until it is handled here. Kinds not yet
/// implemented are filled in their later phase and currently contribute nothing.
fn add_assignment(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    assignment: &FanAssignment,
    preset: Preset,
) -> Result<(), CCError> {
    match assignment.kind {
        FanKind::CpuCooler => {
            let cpu_temp = key_temps.cpu.as_ref().ok_or_else(|| CCError::UserError {
                msg: "A CPU air cooler was assigned but no CPU temp was selected".to_string(),
            })?;
            add_cpu_cooler(proposal, context, assignment, cpu_temp, preset);
        }
        FanKind::GpuFan => {
            let gpu_temp = key_temps.gpu.as_ref().ok_or_else(|| CCError::UserError {
                msg: "A GPU fan was assigned but no GPU temp was selected".to_string(),
            })?;
            add_gpu_fan(proposal, context, assignment, gpu_temp, preset);
        }
        FanKind::CaseIntake => {
            add_case_fan(
                proposal,
                context,
                key_temps,
                assignment,
                preset,
                CaseRole::Intake,
            )?;
        }
        FanKind::CaseExhaust => {
            add_case_fan(
                proposal,
                context,
                key_temps,
                assignment,
                preset,
                CaseRole::Exhaust,
            )?;
        }
        FanKind::AioPump => add_aio_pump(proposal, context, key_temps, assignment, preset)?,
        FanKind::AioRadiator => add_aio_radiator(proposal, context, key_temps, assignment, preset)?,
        FanKind::LaptopFan => add_laptop_fan(proposal, context, key_temps, assignment, preset)?,
    }
    Ok(())
}

/// CPU air cooler: a Graph off the CPU temp. Silent smooths the input via an EMA sensor;
/// Balanced and Performance use downward-only hysteresis. The curve floor is raised to the
/// channel's minimum duty so a low Silent floor never stalls the fan.
fn add_cpu_cooler(
    proposal: &mut Proposal,
    context: &DeviceContext,
    assignment: &FanAssignment,
    cpu_temp: &TempSource,
    preset: Preset,
) {
    let source = resolve_smoothed_source(proposal, context, cpu_temp, preset);
    let function_uid = proposal.intern_function(build_preset_function(preset));
    let min_duty = context.min_duty(&assignment.device_uid, &assignment.channel_name);
    let curve = clamp_curve_floor(cpu_cooler_curve(preset), min_duty);
    assert_valid_curve(&curve);
    let profile = build_graph_profile(
        &format!("CPU Cooler ({preset})"),
        source,
        function_uid,
        curve,
    );
    let profile_uid = proposal.intern_profile(profile);
    proposal.assign(assignment, profile_uid);
}

/// GPU fan: a Graph off the GPU temp. The curve may idle at 0% to preserve the card's
/// zero-RPM behavior, so its floor is NOT raised to the channel minimum.
fn add_gpu_fan(
    proposal: &mut Proposal,
    context: &DeviceContext,
    assignment: &FanAssignment,
    gpu_temp: &TempSource,
    preset: Preset,
) {
    let source = resolve_smoothed_source(proposal, context, gpu_temp, preset);
    let function_uid = proposal.intern_function(build_preset_function(preset));
    let curve = gpu_fan_curve(preset);
    assert_valid_curve(&curve);
    let profile = build_graph_profile(&format!("GPU Fan ({preset})"), source, function_uid, curve);
    let profile_uid = proposal.intern_profile(profile);
    proposal.assign(assignment, profile_uid);
}

/// One of the two case-fan roles. They share a base profile and differ only by the overlay
/// offset applied for positive-pressure bias.
#[derive(Debug, Clone, Copy)]
enum CaseRole {
    Intake,
    Exhaust,
}

/// Case fan: an Overlay on the shared case base (Mix(CPU, GPU) Max, or a CPU graph when there is
/// no GPU temp). Intake follows the base; exhaust runs 15% below it for positive pressure. Both
/// keep a 1% floor so the fan stays addressable at idle. Intake and exhaust share one base via
/// de-duplication.
fn add_case_fan(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    assignment: &FanAssignment,
    preset: Preset,
    role: CaseRole,
) -> Result<(), CCError> {
    let base_uid = build_case_base(proposal, context, key_temps, preset)?;
    let (name, offset_profile) = match role {
        CaseRole::Intake => (format!("Case Intake ({preset})"), intake_offset_profile()),
        CaseRole::Exhaust => (format!("Case Exhaust ({preset})"), exhaust_offset_profile()),
    };
    let overlay = build_overlay_profile(&name, base_uid, offset_profile);
    let profile_uid = proposal.intern_profile(overlay);
    proposal.assign(assignment, profile_uid);
    Ok(())
}

/// The shared base profile case fans overlay onto: Mix(CPU, GPU) Max when a GPU temp exists,
/// otherwise the CPU graph alone. Members carry the per-preset curve and smoothing. Returns the
/// base profile UID (de-duplicated, so intake and exhaust share one base).
fn build_case_base(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    preset: Preset,
) -> Result<ProfileUID, CCError> {
    let cpu_temp = key_temps.cpu.as_ref().ok_or_else(|| CCError::UserError {
        msg: "Case fans were assigned but no CPU temp was selected".to_string(),
    })?;
    let cpu_member_uid = build_case_member(proposal, context, cpu_temp, preset, "CPU");
    let Some(gpu_temp) = key_temps.gpu.as_ref() else {
        return Ok(cpu_member_uid);
    };
    let gpu_member_uid = build_case_member(proposal, context, gpu_temp, preset, "GPU");
    let mix = build_mix_profile(
        &format!("Case Airflow ({preset})"),
        vec![cpu_member_uid, gpu_member_uid],
        ProfileMixFunctionType::Max,
    );
    Ok(proposal.intern_profile(mix))
}

/// A Mix member for case fans: a Graph off the given temp using the case curve and the preset's
/// smoothing. Case members are not floor-clamped to `min_duty`: the 1% overlay floor intentionally
/// allows near-stop at idle.
fn build_case_member(
    proposal: &mut Proposal,
    context: &DeviceContext,
    temp: &TempSource,
    preset: Preset,
    label: &str,
) -> ProfileUID {
    let source = resolve_smoothed_source(proposal, context, temp, preset);
    let function_uid = proposal.intern_function(build_preset_function(preset));
    let curve = case_curve(preset);
    assert_valid_curve(&curve);
    let profile = build_graph_profile(
        &format!("Case {label} ({preset})"),
        source,
        function_uid,
        curve,
    );
    proposal.intern_profile(profile)
}

/// Case fan curves per preset (CPU or GPU temp in Celsius, duty percent). Strawman values
/// pending tuning at the phase checkpoint.
fn case_curve(preset: Preset) -> Vec<(Temp, Duty)> {
    match preset {
        Preset::Silent => vec![(50.0, 20), (65.0, 40), (78.0, 70)],
        Preset::Balanced => vec![(45.0, 30), (65.0, 70), (78.0, 100)],
        Preset::Performance => vec![(40.0, 40), (60.0, 80), (72.0, 100)],
    }
}

/// Intake overlay offset: output = max(base, 1%). Keeps the fan addressable at idle without
/// reducing the airflow the thermal curve asks for.
fn intake_offset_profile() -> Vec<(Duty, Offset)> {
    vec![(0, 1), (1, 0), (100, 0)]
}

/// Exhaust overlay offset: output = max(base - 15%, 1%). Running 15% below the shared thermal
/// demand biases the case toward positive pressure (more intake than exhaust). The breakpoint at
/// duty 16 is where base minus 15 meets the 1% floor.
fn exhaust_offset_profile() -> Vec<(Duty, Offset)> {
    vec![(0, 1), (16, -15), (100, -15)]
}

/// A Mix profile combining member profiles with the given function. Its own function is the
/// default identity: the members already carry their curves and smoothing.
fn build_mix_profile(
    name: &str,
    member_profile_uids: Vec<ProfileUID>,
    mix_function_type: ProfileMixFunctionType,
) -> Profile {
    Profile {
        uid: Uuid::new_v4().to_string(),
        name: name.to_string(),
        function_uid: DEFAULT_FUNCTION_UID.to_string(),
        kind: ProfileKind::Mix {
            member_profile_uids,
            mix_function_type: Some(mix_function_type),
        },
    }
}

/// An Overlay profile applying an offset to a single base profile. Its own function is the
/// default identity: the offset is the transform.
fn build_overlay_profile(
    name: &str,
    base_uid: ProfileUID,
    offset_profile: Vec<(Duty, Offset)>,
) -> Profile {
    Profile {
        uid: Uuid::new_v4().to_string(),
        name: name.to_string(),
        function_uid: DEFAULT_FUNCTION_UID.to_string(),
        kind: ProfileKind::Overlay {
            member_profile_uids: vec![base_uid],
            offset_profile: Some(offset_profile),
        },
    }
}

/// AIO pump: it pulls heat off the CPU die, so it tracks the CPU temp (liquid as fallback).
/// Silent is a quiet 2-step curve with a 50% floor; Balanced and Performance run the pump at a
/// fixed 100% for maximum flow. Only Silent needs a temp source.
fn add_aio_pump(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    assignment: &FanAssignment,
    preset: Preset,
) -> Result<(), CCError> {
    let profile = match preset {
        Preset::Silent => {
            let base = key_temps
                .cpu
                .as_ref()
                .or(key_temps.liquid.as_ref())
                .ok_or_else(|| CCError::UserError {
                    msg: "An AIO pump was assigned but no CPU or liquid temp was selected"
                        .to_string(),
                })?;
            let source = resolve_smoothed_source(proposal, context, base, Preset::Silent);
            let function_uid = proposal.intern_function(build_preset_function(Preset::Silent));
            build_graph_profile(
                &format!("AIO Pump ({preset})"),
                source,
                function_uid,
                pump_silent_curve(),
            )
        }
        Preset::Balanced | Preset::Performance => {
            build_fixed_profile(&format!("AIO Pump ({preset})"), 100)
        }
    };
    let profile_uid = proposal.intern_profile(profile);
    proposal.assign(assignment, profile_uid);
    Ok(())
}

/// The Silent pump's 2-step curve (CPU temp in Celsius, duty percent): a quiet 50% floor that
/// ramps to full flow under load. 50% keeps noise down while still moving coolant.
fn pump_silent_curve() -> Vec<(Temp, Duty)> {
    vec![(50.0, 50), (70.0, 100)]
}

/// The temperature band a radiator curve is shaped for, chosen by the available temp source.
#[derive(Debug, Clone, Copy)]
enum RadiatorBand {
    Delta,
    Liquid,
    Cpu,
}

/// AIO radiator: a Graph off the loop's thermal signal. Prefers a liquid-minus-ambient Delta
/// (created here) when both exist, else the raw liquid temp, else the CPU temp as a fallback.
/// The liquid and Delta signals are slow-moving, so no EMA smoothing is applied.
fn add_aio_radiator(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    assignment: &FanAssignment,
    preset: Preset,
) -> Result<(), CCError> {
    let (source, band) = resolve_radiator_source(proposal, context, key_temps)?;
    let function_uid = proposal.intern_function(build_preset_function(preset));
    let curve = radiator_curve(preset, band);
    assert_valid_curve(&curve);
    let profile = build_graph_profile(
        &format!("AIO Radiator ({preset})"),
        source,
        function_uid,
        curve,
    );
    let profile_uid = proposal.intern_profile(profile);
    proposal.assign(assignment, profile_uid);
    Ok(())
}

/// Chooses the radiator's temp source and matching curve band. A Delta custom sensor is created
/// (and de-duplicated) only when a liquid temp, an ambient temp, and a custom-sensors device are
/// all available.
fn resolve_radiator_source(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
) -> Result<(TempSource, RadiatorBand), CCError> {
    if let (Some(liquid), Some(ambient), Some(custom_sensors_device_uid)) = (
        key_temps.liquid.as_ref(),
        key_temps.ambient.as_ref(),
        context.custom_sensors_device_uid.as_ref(),
    ) {
        let sensor_id =
            proposal.intern_custom_sensor(build_delta_sensor(liquid.clone(), ambient.clone()));
        let source = TempSource {
            temp_name: sensor_id,
            device_uid: custom_sensors_device_uid.clone(),
        };
        return Ok((source, RadiatorBand::Delta));
    }
    if let Some(liquid) = key_temps.liquid.as_ref() {
        return Ok((liquid.clone(), RadiatorBand::Liquid));
    }
    if let Some(cpu) = key_temps.cpu.as_ref() {
        return Ok((cpu.clone(), RadiatorBand::Cpu));
    }
    Err(CCError::UserError {
        msg: "An AIO radiator was assigned but no liquid or CPU temp was selected".to_string(),
    })
}

/// Radiator curves per preset and band. Liquid curves ramp 28C to 38C; Delta curves ramp 5C to
/// 10C; the CPU fallback reuses the wider CPU-cooler band. Strawman values pending tuning.
fn radiator_curve(preset: Preset, band: RadiatorBand) -> Vec<(Temp, Duty)> {
    match band {
        RadiatorBand::Delta => match preset {
            Preset::Silent => vec![(5.0, 30), (10.0, 80)],
            Preset::Balanced => vec![(5.0, 40), (10.0, 100)],
            Preset::Performance => vec![(5.0, 50), (10.0, 100)],
        },
        RadiatorBand::Liquid => match preset {
            Preset::Silent => vec![(28.0, 30), (38.0, 80)],
            Preset::Balanced => vec![(28.0, 40), (38.0, 100)],
            Preset::Performance => vec![(28.0, 50), (38.0, 100)],
        },
        RadiatorBand::Cpu => cpu_cooler_curve(preset),
    }
}

/// A Delta custom sensor giving liquid minus ambient (the loop's thermal load, independent of
/// room temperature). The engine's Delta is the absolute spread between sources, so order does
/// not matter.
fn build_delta_sensor(liquid: TempSource, ambient: TempSource) -> CustomSensor {
    CustomSensor {
        id: format!("Auto Delta {} {}", liquid.temp_name, ambient.temp_name),
        kind: CustomSensorKind::Mix {
            mix_function: CustomSensorMixFunctionType::Delta,
            sources: vec![
                CustomTempSourceData {
                    temp_source: liquid,
                    weight: 1,
                },
                CustomTempSourceData {
                    temp_source: ambient,
                    weight: 1,
                },
            ],
        },
        children: Vec::new(),
        parents: Vec::new(),
    }
}

/// A Fixed profile holding a constant duty. A fresh UID is assigned.
fn build_fixed_profile(name: &str, duty: Duty) -> Profile {
    Profile {
        uid: Uuid::new_v4().to_string(),
        name: name.to_string(),
        function_uid: DEFAULT_FUNCTION_UID.to_string(),
        kind: ProfileKind::Fixed {
            speed_fixed: Some(duty),
        },
    }
}

/// Laptop fan. Laptops run hot and hold heat, so every preset uses downward-only hysteresis and
/// Silent additionally sustains via a long EMA window and a high knee. The temp source follows the
/// chosen strategy: an EMA of the CPU (default), the CPU temp read directly, or a Mix of CPU and
/// GPU (Max, so a powered-off dGPU reading 0C is ignored). A Mix request with no GPU temp degrades
/// to the EMA-CPU default.
fn add_laptop_fan(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    assignment: &FanAssignment,
    preset: Preset,
) -> Result<(), CCError> {
    let strategy = assignment
        .laptop_temp_strategy
        .unwrap_or(LaptopTempStrategy::EmaCpu);
    let profile_uid = match strategy {
        LaptopTempStrategy::MixCpuGpu if key_temps.gpu.is_some() => {
            build_laptop_mix(proposal, context, key_temps, preset)?
        }
        LaptopTempStrategy::ThinkpadSensor => {
            build_laptop_graph(proposal, context, key_temps, preset, false)?
        }
        // EmaCpu, or MixCpuGpu with no GPU temp (degrade to the EMA-CPU default).
        _ => build_laptop_graph(proposal, context, key_temps, preset, true)?,
    };
    proposal.assign(assignment, profile_uid);
    Ok(())
}

/// A laptop fan as a single Graph off the CPU temp, optionally EMA-smoothed.
fn build_laptop_graph(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    preset: Preset,
    smooth: bool,
) -> Result<ProfileUID, CCError> {
    let cpu = key_temps.cpu.as_ref().ok_or_else(|| CCError::UserError {
        msg: "A laptop fan was assigned but no CPU temp was selected".to_string(),
    })?;
    let source = if smooth {
        laptop_ema_source(proposal, context, cpu, preset)
    } else {
        cpu.clone()
    };
    let function_uid = proposal.intern_function(build_laptop_function(preset));
    let curve = laptop_curve(preset);
    assert_valid_curve(&curve);
    let profile = build_graph_profile(
        &format!("Laptop Fan ({preset})"),
        source,
        function_uid,
        curve,
    );
    Ok(proposal.intern_profile(profile))
}

/// A laptop fan as a Mix(CPU, GPU) Max, each member an EMA-smoothed Graph. Used when the user
/// picks the Mix strategy and a GPU temp is available.
fn build_laptop_mix(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    preset: Preset,
) -> Result<ProfileUID, CCError> {
    let cpu = key_temps.cpu.as_ref().ok_or_else(|| CCError::UserError {
        msg: "A laptop fan was assigned but no CPU temp was selected".to_string(),
    })?;
    let gpu = key_temps.gpu.as_ref().ok_or_else(|| CCError::UserError {
        msg: "A laptop Mix source needs a GPU temp".to_string(),
    })?;
    let cpu_member = build_laptop_member(proposal, context, cpu, preset);
    let gpu_member = build_laptop_member(proposal, context, gpu, preset);
    let mix = build_mix_profile(
        &format!("Laptop Mix ({preset})"),
        vec![cpu_member, gpu_member],
        ProfileMixFunctionType::Max,
    );
    Ok(proposal.intern_profile(mix))
}

/// One EMA-smoothed Graph member of a laptop Mix.
fn build_laptop_member(
    proposal: &mut Proposal,
    context: &DeviceContext,
    temp: &TempSource,
    preset: Preset,
) -> ProfileUID {
    let source = laptop_ema_source(proposal, context, temp, preset);
    let function_uid = proposal.intern_function(build_laptop_function(preset));
    let curve = laptop_curve(preset);
    let profile = build_graph_profile(
        &format!("Laptop {} ({preset})", temp.temp_name),
        source,
        function_uid,
        curve,
    );
    proposal.intern_profile(profile)
}

/// Wraps a laptop temp in an EMA sensor sized for the preset (Silent gets the longest window for
/// sustain). Falls back to the raw temp if there is no custom-sensors device.
fn laptop_ema_source(
    proposal: &mut Proposal,
    context: &DeviceContext,
    base: &TempSource,
    preset: Preset,
) -> TempSource {
    let Some(custom_sensors_device_uid) = context.custom_sensors_device_uid.clone() else {
        return base.clone();
    };
    let sensor_id =
        proposal.intern_custom_sensor(build_ema_sensor(base.clone(), laptop_ema_window(preset)));
    TempSource {
        temp_name: sensor_id,
        device_uid: custom_sensors_device_uid,
    }
}

/// Laptop EMA window per preset, in seconds. Silent is long to sustain through brief load before
/// ramping; Performance is short to react quickly. Strawman values pending tuning.
fn laptop_ema_window(preset: Preset) -> u16 {
    match preset {
        Preset::Silent => 30,
        Preset::Balanced => 15,
        Preset::Performance => 10,
    }
}

/// Laptop hysteresis function per preset. All presets use downward-only hysteresis with a large
/// deviance because laptops hold heat (a slow down-ramp avoids surging). Silent is the slowest.
/// Strawman values pending tuning.
fn build_laptop_function(preset: Preset) -> Function {
    let (deviance, response_delay) = match preset {
        Preset::Silent => (5.0, 5),
        Preset::Balanced => (3.0, 3),
        Preset::Performance => (2.0, 1),
    };
    Function {
        uid: Uuid::new_v4().to_string(),
        name: format!("Auto Laptop ({preset})"),
        only_downward: Some(true),
        deviance: Some(deviance),
        response_delay: Some(response_delay),
        ..Function::default()
    }
}

/// Laptop fan curves per preset (CPU temp in Celsius, duty percent). High knees keep the fan
/// quiet until temps are genuinely high, matching how laptops run hot. Strawman values pending
/// tuning.
fn laptop_curve(preset: Preset) -> Vec<(Temp, Duty)> {
    match preset {
        Preset::Silent => vec![(60.0, 20), (80.0, 40), (95.0, 100)],
        Preset::Balanced => vec![(55.0, 30), (75.0, 60), (90.0, 100)],
        Preset::Performance => vec![(50.0, 40), (70.0, 80), (85.0, 100)],
    }
}

/// The temp source a profile should follow. For Silent, the base temp is wrapped in an EMA
/// custom sensor (created and de-duplicated here) so the fan follows a smoothed signal rather
/// than chasing spikes. With no custom-sensors device available, the raw temp is used.
fn resolve_smoothed_source(
    proposal: &mut Proposal,
    context: &DeviceContext,
    base: &TempSource,
    preset: Preset,
) -> TempSource {
    if preset != Preset::Silent {
        return base.clone();
    }
    let Some(custom_sensors_device_uid) = context.custom_sensors_device_uid.clone() else {
        return base.clone();
    };
    let sensor_id =
        proposal.intern_custom_sensor(build_ema_sensor(base.clone(), SILENT_EMA_WINDOW_SECONDS));
    TempSource {
        temp_name: sensor_id,
        device_uid: custom_sensors_device_uid,
    }
}

/// An EMA custom sensor wrapping a single temp source. The id encodes both the source temp name
/// and the window, so fans smoothing the same temp with the same window share one sensor via
/// de-duplication, while differing windows (e.g. the laptop's longer Silent window) stay distinct.
fn build_ema_sensor(source: TempSource, window_seconds: u16) -> CustomSensor {
    CustomSensor {
        id: format!("Auto EMA {} {window_seconds}s", source.temp_name),
        kind: CustomSensorKind::ExponentialMovingAvg {
            time_window_seconds: window_seconds,
            sources: vec![CustomTempSourceData {
                temp_source: source,
                weight: 1,
            }],
        },
        children: Vec::new(),
        parents: Vec::new(),
    }
}

/// The hysteresis function for a preset. Silent relies on EMA smoothing of its source, so it
/// keeps a plain function; Balanced and Performance ramp up promptly but ease down via
/// downward-only hysteresis, Performance being the snappier of the two. Strawman values pending
/// tuning.
fn build_preset_function(preset: Preset) -> Function {
    let name = format!("Auto Fan ({preset})");
    match preset {
        Preset::Silent => Function {
            uid: Uuid::new_v4().to_string(),
            name,
            ..Function::default()
        },
        Preset::Balanced => Function {
            uid: Uuid::new_v4().to_string(),
            name,
            only_downward: Some(true),
            deviance: Some(2.0),
            response_delay: Some(2),
            ..Function::default()
        },
        Preset::Performance => Function {
            uid: Uuid::new_v4().to_string(),
            name,
            only_downward: Some(true),
            deviance: Some(1.0),
            response_delay: Some(1),
            ..Function::default()
        },
    }
}

/// CPU cooler curves per preset (CPU temp in Celsius, duty percent). Strawman values pending
/// tuning at the phase checkpoint.
fn cpu_cooler_curve(preset: Preset) -> Vec<(Temp, Duty)> {
    match preset {
        Preset::Silent => vec![(45.0, 25), (60.0, 40), (75.0, 70), (85.0, 100)],
        Preset::Balanced => vec![(35.0, 30), (55.0, 55), (75.0, 90), (85.0, 100)],
        Preset::Performance => vec![(30.0, 40), (50.0, 70), (65.0, 100)],
    }
}

/// GPU fan curves per preset (GPU temp in Celsius, duty percent). The low-temp 0% entries
/// preserve the card's zero-RPM idle. Strawman values pending tuning.
fn gpu_fan_curve(preset: Preset) -> Vec<(Temp, Duty)> {
    match preset {
        Preset::Silent => vec![(45.0, 0), (55.0, 30), (70.0, 60), (83.0, 90)],
        Preset::Balanced => vec![(45.0, 0), (60.0, 40), (75.0, 80), (85.0, 100)],
        Preset::Performance => vec![(40.0, 30), (55.0, 65), (70.0, 100)],
    }
}

/// Raises every duty in the curve to at least `min_duty` so a low floor cannot stall a fan
/// that needs more to spin. Monotonicity is preserved because the clamp is applied uniformly.
fn clamp_curve_floor(curve: Vec<(Temp, Duty)>, min_duty: Duty) -> Vec<(Temp, Duty)> {
    curve
        .into_iter()
        .map(|(temp, duty)| (temp, duty.max(min_duty)))
        .collect()
}

/// Asserts a generated curve is well-formed: non-empty, temps strictly increasing, duties
/// non-decreasing.
fn assert_valid_curve(curve: &[(Temp, Duty)]) {
    debug_assert!(curve.is_empty().not(), "curve must have points");
    debug_assert!(
        curve.windows(2).all(|w| w[0].0 < w[1].0),
        "curve temps must strictly increase"
    );
    debug_assert!(
        curve.windows(2).all(|w| w[0].1 <= w[1].1),
        "curve duties must not decrease"
    );
}

/// A Graph profile following a single temp source. A fresh UID is assigned.
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

/// The device facts the generator needs: each controllable channel's effective (calibration
/// aware) minimum duty, and the custom-sensors device UID where generated EMA and Delta sensors
/// live so a profile can reference them.
struct DeviceContext {
    min_duty_by_channel: HashMap<ChannelKey, Duty>,
    custom_sensors_device_uid: Option<DeviceUID>,
}

type ChannelKey = (DeviceUID, ChannelName);

impl DeviceContext {
    fn from_devices(devices: &[DeviceDto]) -> Self {
        let mut min_duty_by_channel = HashMap::new();
        let mut custom_sensors_device_uid = None;
        for device in devices {
            if device.d_type == DeviceType::CustomSensors {
                custom_sensors_device_uid = Some(device.uid.clone());
            }
            for (channel_name, channel_info) in &device.info.channels {
                if let Some(speed_options) = channel_info.speed_options() {
                    let key = (device.uid.clone(), channel_name.clone());
                    min_duty_by_channel.insert(key, speed_options.min_duty);
                }
            }
        }
        Self {
            min_duty_by_channel,
            custom_sensors_device_uid,
        }
    }

    /// The channel's effective minimum duty, or 0 when the channel is unknown.
    fn min_duty(&self, device_uid: &str, channel_name: &str) -> Duty {
        let key = (device_uid.to_string(), channel_name.to_string());
        self.min_duty_by_channel.get(&key).copied().unwrap_or(0)
    }
}

/// Accumulates the entities a generation run proposes, de-duplicating custom sensors,
/// functions, and profiles that share an identical definition so the user's lists are not
/// cluttered with copies.
struct Proposal {
    custom_sensors: Vec<CustomSensor>,
    functions: Vec<Function>,
    profiles: Vec<Profile>,
    assignments: Vec<ChannelAssignment>,
    custom_sensor_id_by_signature: HashMap<String, TempName>,
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
            custom_sensor_id_by_signature: HashMap::new(),
            function_uid_by_signature: HashMap::with_capacity(fan_count),
            profile_uid_by_signature: HashMap::with_capacity(fan_count),
        }
    }

    /// Returns the id of an already-proposed identical custom sensor, or stores this one and
    /// returns its id.
    fn intern_custom_sensor(&mut self, sensor: CustomSensor) -> TempName {
        let signature = custom_sensor_signature(&sensor);
        if let Some(existing_id) = self.custom_sensor_id_by_signature.get(&signature) {
            return existing_id.clone();
        }
        let id = sensor.id.clone();
        self.custom_sensor_id_by_signature
            .insert(signature, id.clone());
        self.custom_sensors.push(sensor);
        id
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

    /// Records that a fan channel should be assigned the given profile.
    fn assign(&mut self, assignment: &FanAssignment, profile_uid: ProfileUID) {
        self.assignments.push(ChannelAssignment {
            device_uid: assignment.device_uid.clone(),
            channel_name: assignment.channel_name.clone(),
            profile_uid,
            replaces_profile_name: None,
        });
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

/// A definition fingerprint of a custom sensor, including its id and kind, so duplicate
/// definitions collapse to one.
fn custom_sensor_signature(sensor: &CustomSensor) -> String {
    format!("{}|{:?}", sensor.id, sensor.kind)
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

    fn gpu_temp() -> TempSource {
        TempSource {
            temp_name: "GPU Temp".to_string(),
            device_uid: "dev-gpu-1".to_string(),
        }
    }

    fn gpu_request(channel_names: &[&str], preset: Preset) -> GenerateProfilesRequest {
        let assignments = channel_names
            .iter()
            .map(|channel_name| FanAssignment {
                device_uid: "dev-mb-1".to_string(),
                channel_name: (*channel_name).to_string(),
                kind: FanKind::GpuFan,
                position: None,
                laptop_temp_strategy: None,
            })
            .collect();
        GenerateProfilesRequest {
            assignments,
            key_temps: KeyTemps {
                cpu: None,
                gpu: Some(gpu_temp()),
                liquid: None,
                ambient: None,
            },
            global_preset: preset,
            preset_overrides: Vec::new(),
        }
    }

    /// A context with a custom-sensors device present and no per-channel minimum duties.
    fn test_context() -> DeviceContext {
        DeviceContext {
            min_duty_by_channel: HashMap::new(),
            custom_sensors_device_uid: Some("dev-custom-sensors".to_string()),
        }
    }

    /// A context that reports one channel's effective minimum duty.
    fn context_with_min_duty(
        device_uid: &str,
        channel_name: &str,
        min_duty: Duty,
    ) -> DeviceContext {
        let mut min_duty_by_channel = HashMap::new();
        min_duty_by_channel.insert((device_uid.to_string(), channel_name.to_string()), min_duty);
        DeviceContext {
            min_duty_by_channel,
            custom_sensors_device_uid: Some("dev-custom-sensors".to_string()),
        }
    }

    /// Goal: a single CPU cooler yields a valid Graph profile plus its function, wired
    /// together and assigned to the fan. Method: generate, then assert the entity counts, the
    /// profile shape, and that the assignment points at the produced profile.
    #[test]
    fn generates_cpu_cooler_profile_and_function() {
        let response = generate_proposal(
            &cpu_cooler_request(&["fan1"], Preset::Balanced),
            &test_context(),
        )
        .expect("generates");
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
        let response = generate_proposal(
            &cpu_cooler_request(&["fan1", "fan2"], Preset::Balanced),
            &test_context(),
        )
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
        assert!(generate_proposal(&request, &test_context()).is_err());
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
        let response = generate_proposal(&request, &test_context()).expect("generates");
        assert!(response.profiles.is_empty());
        assert!(response.functions.is_empty());
        assert!(response.custom_sensors.is_empty());
        assert!(response.assignments.is_empty());
    }

    /// Goal: the Silent preset wraps the temp in an EMA custom sensor and points the profile at
    /// it. Method: generate a Silent CPU cooler and assert one EMA sensor exists and the
    /// profile's source is that sensor on the custom-sensors device.
    #[test]
    fn cpu_cooler_silent_wraps_source_in_ema_sensor() {
        let response = generate_proposal(
            &cpu_cooler_request(&["fan1"], Preset::Silent),
            &test_context(),
        )
        .expect("generates");
        assert_eq!(response.custom_sensors.len(), 1);
        let sensor = &response.custom_sensors[0];
        assert!(matches!(
            sensor.kind,
            CustomSensorKind::ExponentialMovingAvg { .. }
        ));
        let source = response.profiles[0]
            .temp_source()
            .expect("graph has a source");
        assert_eq!(source.device_uid, "dev-custom-sensors");
        assert_eq!(source.temp_name, sensor.id);
    }

    /// Goal: non-Silent presets follow the raw temp with no EMA sensor. Method: generate a
    /// Balanced CPU cooler and assert no custom sensors and the raw CPU source.
    #[test]
    fn cpu_cooler_balanced_uses_raw_temp() {
        let response = generate_proposal(
            &cpu_cooler_request(&["fan1"], Preset::Balanced),
            &test_context(),
        )
        .expect("generates");
        assert!(response.custom_sensors.is_empty());
        assert_eq!(response.profiles[0].temp_source(), Some(&cpu_temp()));
    }

    /// Goal: a channel minimum duty raises the CPU cooler curve floor so it cannot stall.
    /// Method: generate with `min_duty` 40 and assert every curve duty is at least 40.
    #[test]
    fn cpu_cooler_curve_floor_clamped_to_min_duty() {
        let context = context_with_min_duty("dev-mb-1", "fan1", 40);
        let response = generate_proposal(&cpu_cooler_request(&["fan1"], Preset::Silent), &context)
            .expect("generates");
        let curve = response.profiles[0].speed_profile().expect("has curve");
        assert!(curve.iter().all(|(_, duty)| *duty >= 40));
    }

    /// Goal: GPU fans keep a 0% idle even when the channel reports a non-zero minimum duty, to
    /// preserve zero-RPM. Method: generate with `min_duty` 30 and assert the curve still has a 0%
    /// point.
    #[test]
    fn gpu_fan_preserves_zero_rpm_idle() {
        let context = context_with_min_duty("dev-mb-1", "fan1", 30);
        let response = generate_proposal(&gpu_request(&["fan1"], Preset::Silent), &context)
            .expect("generates");
        let curve = response.profiles[0].speed_profile().expect("has curve");
        assert!(
            curve.iter().any(|(_, duty)| *duty == 0),
            "zero-RPM idle preserved"
        );
    }

    /// Goal: assigning a GPU fan without a GPU temp is a user error. Method: omit the GPU temp
    /// and assert generation errors.
    #[test]
    fn gpu_fan_without_gpu_temp_errors() {
        let mut request = gpu_request(&["fan1"], Preset::Balanced);
        request.key_temps.gpu = None;
        assert!(generate_proposal(&request, &test_context()).is_err());
    }

    fn case_request(preset: Preset, with_gpu: bool) -> GenerateProfilesRequest {
        GenerateProfilesRequest {
            assignments: vec![
                FanAssignment {
                    device_uid: "dev-mb-1".to_string(),
                    channel_name: "fan2".to_string(),
                    kind: FanKind::CaseIntake,
                    position: Some(FanPosition::Front),
                    laptop_temp_strategy: None,
                },
                FanAssignment {
                    device_uid: "dev-mb-1".to_string(),
                    channel_name: "fan3".to_string(),
                    kind: FanKind::CaseExhaust,
                    position: Some(FanPosition::Back),
                    laptop_temp_strategy: None,
                },
            ],
            key_temps: KeyTemps {
                cpu: Some(cpu_temp()),
                gpu: with_gpu.then(gpu_temp),
                liquid: None,
                ambient: None,
            },
            global_preset: preset,
            preset_overrides: Vec::new(),
        }
    }

    fn count_p_type(profiles: &[Profile], p_type: &ProfileType) -> usize {
        profiles.iter().filter(|p| &p.p_type() == p_type).count()
    }

    /// Goal: with a GPU temp, case fans produce two member graphs, one Max Mix base, and two
    /// Overlays, and each fan is assigned. Method: generate and assert the per-type profile
    /// counts and the assignment count.
    #[test]
    fn case_fans_build_mix_base_and_two_overlays() {
        let response = generate_proposal(&case_request(Preset::Balanced, true), &test_context())
            .expect("generates");
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Graph), 2);
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Mix), 1);
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Overlay), 2);
        assert_eq!(response.assignments.len(), 2);
    }

    /// Goal: intake and exhaust overlay the SAME base, and exhaust carries a negative offset for
    /// positive pressure while intake never does. Method: generate, then assert both overlays
    /// reference the Mix and check the offset signs.
    #[test]
    fn case_intake_and_exhaust_share_base_with_pressure_bias() {
        let response = generate_proposal(&case_request(Preset::Balanced, true), &test_context())
            .expect("generates");
        let mix = response
            .profiles
            .iter()
            .find(|p| p.p_type() == ProfileType::Mix)
            .expect("has a mix base");
        for overlay in response
            .profiles
            .iter()
            .filter(|p| p.p_type() == ProfileType::Overlay)
        {
            assert_eq!(overlay.member_profile_uids(), [mix.uid.clone()].as_slice());
        }
        let exhaust = response
            .profiles
            .iter()
            .find(|p| p.name.contains("Exhaust"))
            .expect("has exhaust");
        assert!(
            exhaust
                .offset_profile()
                .unwrap()
                .iter()
                .any(|(_, off)| *off < 0),
            "exhaust runs below the base for positive pressure"
        );
        let intake = response
            .profiles
            .iter()
            .find(|p| p.name.contains("Intake"))
            .expect("has intake");
        assert!(
            intake
                .offset_profile()
                .unwrap()
                .iter()
                .all(|(_, off)| *off >= 0),
            "intake never runs below the base"
        );
    }

    /// Goal: without a GPU temp, the base degrades to a single CPU graph (no Mix), still with two
    /// overlays referencing it. Method: generate without a GPU temp and assert the shape.
    #[test]
    fn case_fans_degrade_to_cpu_graph_without_gpu() {
        let response = generate_proposal(&case_request(Preset::Balanced, false), &test_context())
            .expect("generates");
        assert_eq!(
            count_p_type(&response.profiles, &ProfileType::Mix),
            0,
            "no GPU temp means no Mix"
        );
        assert_eq!(
            count_p_type(&response.profiles, &ProfileType::Graph),
            1,
            "single CPU base graph"
        );
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Overlay), 2);
        let graph = response
            .profiles
            .iter()
            .find(|p| p.p_type() == ProfileType::Graph)
            .expect("has the base graph");
        for overlay in response
            .profiles
            .iter()
            .filter(|p| p.p_type() == ProfileType::Overlay)
        {
            assert_eq!(
                overlay.member_profile_uids(),
                [graph.uid.clone()].as_slice()
            );
        }
    }

    /// Goal: case fans need a CPU temp. Method: omit the CPU temp and assert generation errors.
    #[test]
    fn case_fans_without_cpu_temp_errors() {
        let mut request = case_request(Preset::Balanced, true);
        request.key_temps.cpu = None;
        assert!(generate_proposal(&request, &test_context()).is_err());
    }

    /// Goal: case intake and exhaust must share a preset. Method: override them to different
    /// presets and assert generation errors.
    #[test]
    fn case_preset_coupling_conflict_errors() {
        let mut request = case_request(Preset::Balanced, true);
        request.preset_overrides = vec![
            PresetOverride {
                kind: FanKind::CaseIntake,
                preset: Preset::Silent,
            },
            PresetOverride {
                kind: FanKind::CaseExhaust,
                preset: Preset::Performance,
            },
        ];
        assert!(generate_proposal(&request, &test_context()).is_err());
    }

    /// Goal: Silent case fans smooth both members, creating one EMA sensor per temp. Method:
    /// generate Silent with CPU and GPU temps and assert two EMA sensors.
    #[test]
    fn case_fans_silent_create_ema_per_member() {
        let response = generate_proposal(&case_request(Preset::Silent, true), &test_context())
            .expect("generates");
        assert_eq!(
            response.custom_sensors.len(),
            2,
            "one EMA sensor for CPU, one for GPU"
        );
        assert!(response
            .custom_sensors
            .iter()
            .all(|s| matches!(s.kind, CustomSensorKind::ExponentialMovingAvg { .. })));
    }

    fn liquid_temp() -> TempSource {
        TempSource {
            temp_name: "Liquid".to_string(),
            device_uid: "dev-aio-1".to_string(),
        }
    }

    fn ambient_temp() -> TempSource {
        TempSource {
            temp_name: "Ambient".to_string(),
            device_uid: "dev-mb-1".to_string(),
        }
    }

    fn aio_request(kind: FanKind, preset: Preset, key_temps: KeyTemps) -> GenerateProfilesRequest {
        GenerateProfilesRequest {
            assignments: vec![FanAssignment {
                device_uid: "dev-aio-1".to_string(),
                channel_name: "fan1".to_string(),
                kind,
                position: None,
                laptop_temp_strategy: None,
            }],
            key_temps,
            global_preset: preset,
            preset_overrides: Vec::new(),
        }
    }

    fn cpu_only() -> KeyTemps {
        KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: None,
            liquid: None,
            ambient: None,
        }
    }

    /// Goal: the pump runs at a fixed 100% on Balanced and Performance. Method: generate each and
    /// assert a single Fixed profile at 100% with no smoothing sensor.
    #[test]
    fn aio_pump_balanced_and_performance_are_fixed_full() {
        for preset in [Preset::Balanced, Preset::Performance] {
            let response = generate_proposal(
                &aio_request(FanKind::AioPump, preset, cpu_only()),
                &test_context(),
            )
            .expect("generates");
            assert_eq!(response.profiles.len(), 1);
            let pump = &response.profiles[0];
            assert_eq!(pump.p_type(), ProfileType::Fixed);
            assert_eq!(pump.speed_fixed(), Some(100));
            assert!(response.custom_sensors.is_empty());
        }
    }

    /// Goal: the Silent pump is a 2-step graph with a 50% floor, smoothed off the CPU temp.
    /// Method: generate and assert the curve points and that an EMA sensor was created.
    #[test]
    fn aio_pump_silent_is_two_step_graph() {
        let response = generate_proposal(
            &aio_request(FanKind::AioPump, Preset::Silent, cpu_only()),
            &test_context(),
        )
        .expect("generates");
        let pump = &response.profiles[0];
        assert_eq!(pump.p_type(), ProfileType::Graph);
        assert_eq!(
            pump.speed_profile().expect("has curve").as_slice(),
            &[(50.0, 50), (70.0, 100)]
        );
        assert_eq!(
            response.custom_sensors.len(),
            1,
            "Silent smooths the CPU temp"
        );
    }

    /// Goal: the pump falls back to the liquid temp when no CPU temp is selected. Method:
    /// generate Silent with only a liquid temp and assert the smoothing sensor wraps it.
    #[test]
    fn aio_pump_falls_back_to_liquid() {
        let key_temps = KeyTemps {
            cpu: None,
            gpu: None,
            liquid: Some(liquid_temp()),
            ambient: None,
        };
        let response = generate_proposal(
            &aio_request(FanKind::AioPump, Preset::Silent, key_temps),
            &test_context(),
        )
        .expect("generates");
        assert_eq!(response.custom_sensors.len(), 1);
        assert!(response.custom_sensors[0]
            .sources()
            .iter()
            .any(|s| s.temp_source == liquid_temp()));
    }

    /// Goal: a pump with neither CPU nor liquid temp is a user error. Method: omit both and
    /// assert generation errors.
    #[test]
    fn aio_pump_without_temp_errors() {
        let empty = KeyTemps {
            cpu: None,
            gpu: None,
            liquid: None,
            ambient: None,
        };
        assert!(generate_proposal(
            &aio_request(FanKind::AioPump, Preset::Silent, empty),
            &test_context()
        )
        .is_err());
    }

    /// Goal: with a liquid temp but no ambient, the radiator follows the raw liquid in the 28C to
    /// 38C band with no Delta sensor. Method: generate and assert the source and band.
    #[test]
    fn aio_radiator_off_liquid_band() {
        let key_temps = KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: None,
            liquid: Some(liquid_temp()),
            ambient: None,
        };
        let response = generate_proposal(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, key_temps),
            &test_context(),
        )
        .expect("generates");
        let radiator = &response.profiles[0];
        assert_eq!(radiator.temp_source(), Some(&liquid_temp()));
        assert_eq!(
            radiator.speed_profile().expect("has curve").as_slice(),
            &[(28.0, 40), (38.0, 100)]
        );
        assert!(response.custom_sensors.is_empty());
    }

    /// Goal: with both liquid and ambient temps, the radiator follows an auto-created Delta sensor
    /// in the 5C to 10C band. Method: generate and assert the Delta sensor, source, and band.
    #[test]
    fn aio_radiator_off_delta_creates_sensor() {
        let key_temps = KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: None,
            liquid: Some(liquid_temp()),
            ambient: Some(ambient_temp()),
        };
        let response = generate_proposal(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, key_temps),
            &test_context(),
        )
        .expect("generates");
        assert_eq!(response.custom_sensors.len(), 1);
        let sensor = &response.custom_sensors[0];
        assert!(matches!(
            sensor.kind,
            CustomSensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Delta,
                ..
            }
        ));
        let radiator = &response.profiles[0];
        let source = radiator.temp_source().expect("has source");
        assert_eq!(source.device_uid, "dev-custom-sensors");
        assert_eq!(source.temp_name, sensor.id);
        assert_eq!(
            radiator.speed_profile().expect("has curve").as_slice(),
            &[(5.0, 40), (10.0, 100)]
        );
    }

    /// Goal: with only a CPU temp, the radiator falls back to the CPU-cooler curve. Method:
    /// generate and assert the source is the CPU temp and the curve matches the CPU band.
    #[test]
    fn aio_radiator_falls_back_to_cpu() {
        let response = generate_proposal(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, cpu_only()),
            &test_context(),
        )
        .expect("generates");
        let radiator = &response.profiles[0];
        assert_eq!(radiator.temp_source(), Some(&cpu_temp()));
        assert_eq!(
            radiator.speed_profile().unwrap(),
            &cpu_cooler_curve(Preset::Balanced)
        );
    }

    /// Goal: a radiator with no liquid or CPU temp is a user error. Method: omit both and assert
    /// generation errors.
    #[test]
    fn aio_radiator_without_temp_errors() {
        let empty = KeyTemps {
            cpu: None,
            gpu: None,
            liquid: None,
            ambient: None,
        };
        assert!(generate_proposal(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, empty),
            &test_context()
        )
        .is_err());
    }

    fn laptop_request(
        preset: Preset,
        strategy: Option<LaptopTempStrategy>,
        key_temps: KeyTemps,
    ) -> GenerateProfilesRequest {
        GenerateProfilesRequest {
            assignments: vec![FanAssignment {
                device_uid: "dev-laptop-1".to_string(),
                channel_name: "fan1".to_string(),
                kind: FanKind::LaptopFan,
                position: None,
                laptop_temp_strategy: strategy,
            }],
            key_temps,
            global_preset: preset,
            preset_overrides: Vec::new(),
        }
    }

    /// Goal: the default laptop strategy is EMA of the CPU, with a downward-only function.
    /// Method: generate with no explicit strategy and assert an EMA sensor source plus
    /// `only_downward`.
    #[test]
    fn laptop_default_uses_ema_cpu() {
        let response = generate_proposal(
            &laptop_request(Preset::Balanced, None, cpu_only()),
            &test_context(),
        )
        .expect("generates");
        assert_eq!(response.custom_sensors.len(), 1);
        assert!(matches!(
            response.custom_sensors[0].kind,
            CustomSensorKind::ExponentialMovingAvg { .. }
        ));
        let laptop = &response.profiles[0];
        assert_eq!(laptop.p_type(), ProfileType::Graph);
        assert_eq!(
            laptop.temp_source().unwrap().device_uid,
            "dev-custom-sensors"
        );
        assert_eq!(response.functions[0].only_downward, Some(true));
    }

    /// Goal: the ThinkPad-sensor strategy follows the raw CPU temp with no EMA sensor. Method:
    /// generate with that strategy and assert no custom sensors and the raw CPU source.
    #[test]
    fn laptop_thinkpad_sensor_uses_raw_cpu() {
        let response = generate_proposal(
            &laptop_request(
                Preset::Balanced,
                Some(LaptopTempStrategy::ThinkpadSensor),
                cpu_only(),
            ),
            &test_context(),
        )
        .expect("generates");
        assert!(response.custom_sensors.is_empty());
        assert_eq!(response.profiles[0].temp_source(), Some(&cpu_temp()));
    }

    /// Goal: the Silent laptop uses a longer EMA window than other kinds, to sustain before
    /// ramping. Method: generate Silent and assert the window exceeds the default Silent window.
    #[test]
    fn laptop_silent_uses_long_ema_window() {
        let response = generate_proposal(
            &laptop_request(Preset::Silent, None, cpu_only()),
            &test_context(),
        )
        .expect("generates");
        let CustomSensorKind::ExponentialMovingAvg {
            time_window_seconds,
            ..
        } = response.custom_sensors[0].kind
        else {
            panic!("expected an EMA sensor");
        };
        assert!(
            time_window_seconds > SILENT_EMA_WINDOW_SECONDS,
            "laptop Silent sustains with a longer window"
        );
    }

    /// Goal: the Mix strategy with a GPU temp builds a Mix(CPU, GPU) of two smoothed members.
    /// Method: generate with the Mix strategy and assert one Mix, two member graphs, two EMA
    /// sensors.
    #[test]
    fn laptop_mix_strategy_builds_mix_when_gpu_present() {
        let key_temps = KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: Some(gpu_temp()),
            liquid: None,
            ambient: None,
        };
        let response = generate_proposal(
            &laptop_request(
                Preset::Balanced,
                Some(LaptopTempStrategy::MixCpuGpu),
                key_temps,
            ),
            &test_context(),
        )
        .expect("generates");
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Mix), 1);
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Graph), 2);
        assert_eq!(response.custom_sensors.len(), 2);
    }

    /// Goal: the Mix strategy degrades to the EMA-CPU graph when there is no GPU temp. Method:
    /// generate the Mix strategy with only a CPU temp and assert no Mix profile.
    #[test]
    fn laptop_mix_strategy_degrades_without_gpu() {
        let response = generate_proposal(
            &laptop_request(
                Preset::Balanced,
                Some(LaptopTempStrategy::MixCpuGpu),
                cpu_only(),
            ),
            &test_context(),
        )
        .expect("generates");
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Mix), 0);
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Graph), 1);
    }

    /// Goal: a laptop fan needs a CPU temp. Method: omit it and assert generation errors.
    #[test]
    fn laptop_without_cpu_temp_errors() {
        let empty = KeyTemps {
            cpu: None,
            gpu: None,
            liquid: None,
            ambient: None,
        };
        assert!(generate_proposal(
            &laptop_request(Preset::Balanced, None, empty),
            &test_context()
        )
        .is_err());
    }
}
