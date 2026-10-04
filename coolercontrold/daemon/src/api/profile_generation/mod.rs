// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
    CustomSensor, CustomSensorKind, CustomSensorMixFunctionType, Function, FunctionKind,
    FunctionUID, Offset, Profile, ProfileKind, ProfileMixFunctionType, ProfileUID, SensorSource,
    TempSource, DEFAULT_FUNCTION_UID,
};
use axum::extract::State;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use strum::{Display, EnumString};
use uuid::Uuid;

mod tuning;

use tuning::{SetupEntry, SmoothingSpec, MIN_CURVE_POINTS, MIN_CURVE_TEMP_SPREAD, TUNING};

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

/// Proposes profiles, functions, and custom sensors for the assigned fans, without persisting
/// anything. The UI previews the result and the user confirms before it is saved.
pub async fn generate(
    State(AppState {
        device_handle,
        calibration_handle,
        profile_handle,
        function_handle,
        custom_sensor_handle,
        ..
    }): State<AppState>,
    Json(request): Json<GenerateProfilesRequest>,
) -> Result<Json<GenerateProfilesResponse>, CCError> {
    let mut devices = device_handle.devices_get().await?;
    let calibration_map = build_calibration_map(&calibration_handle).await;
    apply_effective_speed_options(&mut devices, &calibration_map);
    let calibrated_channels = calibration_map
        .iter()
        .filter(|(_, calibration)| calibration.true_to_device(100).is_some())
        .map(|(key, _)| key.clone())
        .collect();
    let context = DeviceContext::from_devices(&devices, calibrated_channels);
    let profiles = profile_handle.get_all().await?;
    let functions = function_handle.get_all().await?;
    let custom_sensors = custom_sensor_handle.get_all().await?;
    let existing = Existing::from_entities(&profiles, &functions, &custom_sensors);
    generate_proposal(&request, &context, &existing).map(Json)
}

/// Builds the proposed entity set for a request. Pure and synchronous: given the request, a device
/// snapshot and what already exists it is fully unit-testable without the daemon's live state.
fn generate_proposal(
    request: &GenerateProfilesRequest,
    context: &DeviceContext,
    existing: &Existing,
) -> Result<GenerateProfilesResponse, CCError> {
    validate_case_preset_coupling(request)?;
    let mut proposal = Proposal::with_capacity(request.assignments.len(), existing);
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

/// The display name of a generated profile: the kind, its preset, and a marker when the channel
/// reads duties on the calibrated scale. The marker is what keeps a calibrated fan from being
/// handed a raw-scale profile by reuse-by-name on a later run.
fn profile_name(label: &str, preset: Preset, duty: ChannelDuty) -> String {
    if duty.calibrated {
        format!("{label} ({preset}, Calibrated)")
    } else {
        format!("{label} ({preset})")
    }
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
    let entry = TUNING.cpu_cooler.get(preset);
    let duty = context.duty_for(assignment, true);
    let profile_uid = build_from_entry(
        proposal,
        context,
        entry,
        cpu_temp,
        &profile_name("CPU Cooler", preset, duty),
        duty,
    );
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
    let entry = TUNING.gpu_fan.get(preset);
    let duty = context.duty_for(assignment, false);
    let profile_uid = build_from_entry(
        proposal,
        context,
        entry,
        gpu_temp,
        &profile_name("GPU Fan", preset, duty),
        duty,
    );
    proposal.assign(assignment, profile_uid);
}

/// One of the two case-fan roles. They share a base profile and differ only by the overlay
/// offset applied for positive-pressure bias.
#[derive(Debug, Clone, Copy)]
enum CaseRole {
    Intake,
    Exhaust,
}

/// Case fan: the shared case base (Mix(CPU, GPU) Max, or a CPU graph when there is no GPU temp).
/// Intake is assigned that base directly; exhaust gets an Overlay running a static
/// `[case.pressure]` bias below it, for positive pressure. Both roles resolve to the same base via
/// de-duplication.
fn add_case_fan(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    assignment: &FanAssignment,
    preset: Preset,
    role: CaseRole,
) -> Result<(), CCError> {
    // Case members idle near stop, so the floor is the exhaust overlay's, not the channel's.
    let duty = context.duty_for(assignment, false);
    let base_uid = build_case_base(proposal, context, key_temps, preset, duty)?;
    let profile_uid = match role {
        // Intake IS the shared airflow demand, so it takes the base as it stands. Wrapping it in
        // an overlay that only lifted a 0% base would add an entity the user has to look through
        // to reach the curve, for a floor the member curves already sit well above.
        CaseRole::Intake => base_uid,
        CaseRole::Exhaust => {
            let pressure = &TUNING.case.pressure;
            let overlay = build_overlay_profile(
                &profile_name("Case Exhaust", preset, duty),
                base_uid,
                exhaust_offset_profile(pressure.exhaust_bias_percent),
            );
            proposal.intern_profile(overlay)
        }
    };
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
    duty: ChannelDuty,
) -> Result<ProfileUID, CCError> {
    let cpu_temp = key_temps.cpu.as_ref().ok_or_else(|| CCError::UserError {
        msg: "Case fans were assigned but no CPU temp was selected".to_string(),
    })?;
    let cpu_member_uid = build_case_member(proposal, context, cpu_temp, preset, "CPU", duty);
    let Some(gpu_temp) = key_temps.gpu.as_ref() else {
        return Ok(cpu_member_uid);
    };
    let gpu_member_uid = build_case_member(proposal, context, gpu_temp, preset, "GPU", duty);
    let mix = build_mix_profile(
        &profile_name("Case Airflow", preset, duty),
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
    duty: ChannelDuty,
) -> ProfileUID {
    let entry = TUNING.case.member.get(preset);
    build_from_entry(
        proposal,
        context,
        entry,
        temp,
        &profile_name(&format!("Case {label}"), preset, duty),
        duty,
    )
}

/// Exhaust overlay offset: output = base - bias%. Running `bias` below the shared thermal demand
/// biases the case toward positive pressure (more intake than exhaust). A single pair is the
/// static-offset encoding: the engine returns that offset for any duty, and the editor shows it as
/// one knob rather than a curve to decode. The engine clamps the sum, so a base below the bias
/// stops the fan rather than going negative.
fn exhaust_offset_profile(bias: Duty) -> Vec<(Duty, Offset)> {
    let bias_offset =
        Offset::try_from(bias).expect("exhaust_bias_percent is validated <= 100, fits i8");
    vec![(STATIC_OFFSET_DUTY, -bias_offset)]
}

/// The duty coordinate a static overlay offset is stored at. Any value works, since a one-pair
/// profile returns its offset for every duty; the UI writes 50, so generated profiles match what
/// the editor produces.
const STATIC_OFFSET_DUTY: Duty = 50;

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
    let entry = TUNING.aio_pump.get(preset);
    let duty = context.duty_for(assignment, true);
    let name = profile_name("AIO Pump", preset, duty);
    let profile_uid = match entry {
        SetupEntry::Fixed { duty: fixed } => {
            proposal.intern_profile(build_fixed_profile(&name, duty.apply_to_duty(*fixed)))
        }
        SetupEntry::Graph { .. } => {
            let base = key_temps
                .cpu
                .as_ref()
                .or(key_temps.liquid.as_ref())
                .ok_or_else(|| CCError::UserError {
                    msg: "An AIO pump was assigned but no CPU or liquid temp was selected"
                        .to_string(),
                })?;
            build_from_entry(proposal, context, entry, base, &name, duty)
        }
    };
    proposal.assign(assignment, profile_uid);
    Ok(())
}

/// The temperature band a radiator curve is shaped for, chosen by the available temp source.
#[derive(Debug, Clone, Copy)]
enum RadiatorBand {
    Delta,
    Liquid,
    Cpu,
}

impl RadiatorBand {
    /// Names the signal in a Mix member's profile name, so the entity list says what it follows.
    fn label(self) -> &'static str {
        match self {
            Self::Delta => "Delta",
            Self::Liquid => "Liquid",
            Self::Cpu => "CPU",
        }
    }
}

/// AIO radiator: a Graph off the loop's thermal signal. Prefers a liquid-minus-ambient Delta
/// (created here) when both exist, else the raw liquid temp, else the CPU temp as a fallback.
/// The liquid and Delta signals are slow-moving, so no EMA smoothing is applied.
///
/// Selecting a GPU temp makes the result a Mix(loop, GPU) Max instead: the loop never carries GPU
/// heat, but radiator fans move the case air that does. The choice is the user's, made by picking
/// (or leaving out) a GPU temp, so no system state is consulted to decide it.
fn add_aio_radiator(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    assignment: &FanAssignment,
    preset: Preset,
) -> Result<(), CCError> {
    let duty = context.duty_for(assignment, true);
    let gpu_temp = key_temps.gpu.as_ref();
    let loop_uid = build_radiator_loop(
        proposal,
        context,
        key_temps,
        preset,
        duty,
        gpu_temp.is_some(),
    )?;
    let profile_uid = match gpu_temp {
        Some(gpu_temp) => build_radiator_mix(proposal, context, gpu_temp, preset, duty, loop_uid),
        None => loop_uid,
    };
    proposal.assign(assignment, profile_uid);
    Ok(())
}

/// The loop half of a radiator: the Graph over the resolved Delta/liquid/CPU source. It carries
/// the channel's minimum-duty floor, so the Mix's Max output can never fall below it whatever the
/// GPU member contributes. `mixes_gpu` only picks the name: standing alone it is the profile the
/// fan is assigned, mixed it is a member and says which signal it follows.
fn build_radiator_loop(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    preset: Preset,
    duty: ChannelDuty,
    mixes_gpu: bool,
) -> Result<ProfileUID, CCError> {
    let (source, band) = resolve_radiator_source(proposal, context, key_temps)?;
    let entry = match band {
        RadiatorBand::Delta => TUNING.aio_radiator.delta.get(preset),
        RadiatorBand::Liquid => TUNING.aio_radiator.liquid.get(preset),
        RadiatorBand::Cpu => TUNING.cpu_cooler.get(preset),
    };
    let label = if mixes_gpu {
        format!("AIO Radiator {}", band.label())
    } else {
        "AIO Radiator".to_string()
    };
    let name = profile_name(&label, preset, duty);
    // Radiator follows a raw source (liquid and Delta are already slow-moving), so the cpu_cooler
    // fallback's smoothing is intentionally bypassed via build_entry_with_source.
    Ok(build_entry_with_source(
        proposal, entry, source, &name, duty,
    ))
}

/// Mix(loop, GPU) Max for a radiator when the user selected a GPU temp. The member reuses the
/// `gpu_fan` curve, which is the one shaped to contribute nothing until the card is genuinely hot:
/// it opens at 0%, so an idle card leaves the loop curve in charge of the fan. It is NOT
/// floor-clamped, which would defeat that.
fn build_radiator_mix(
    proposal: &mut Proposal,
    context: &DeviceContext,
    gpu_temp: &TempSource,
    preset: Preset,
    duty: ChannelDuty,
    loop_uid: ProfileUID,
) -> ProfileUID {
    // The member must be able to idle below the loop curve, so it does not take the channel floor.
    let member_duty = ChannelDuty {
        floor: None,
        calibrated: duty.calibrated,
    };
    let gpu_uid = build_from_entry(
        proposal,
        context,
        TUNING.gpu_fan.get(preset),
        gpu_temp,
        &profile_name("AIO Radiator GPU", preset, member_duty),
        member_duty,
    );
    let mix = build_mix_profile(
        &profile_name("AIO Radiator", preset, duty),
        vec![loop_uid, gpu_uid],
        ProfileMixFunctionType::Max,
    );
    proposal.intern_profile(mix)
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

/// A Delta custom sensor giving liquid minus ambient (the loop's thermal load, independent of
/// room temperature). The engine's Delta is the absolute spread between sources, so order does
/// not matter.
fn build_delta_sensor(liquid: TempSource, ambient: TempSource) -> CustomSensor {
    CustomSensor {
        id: format!("Auto Delta {} {}", liquid.temp_name, ambient.temp_name),
        kind: CustomSensorKind::Mix {
            mix_function: CustomSensorMixFunctionType::Delta,
            sources: vec![
                SensorSource::from_temp(liquid, 1),
                SensorSource::from_temp(ambient, 1),
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
/// the quieter ones sustain via a short EMA window and a high knee. The temp source follows the
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
    let duty = context.duty_for(assignment, laptop_holds_floor(preset));
    let profile_uid = match strategy {
        LaptopTempStrategy::MixCpuGpu if key_temps.gpu.is_some() => {
            build_laptop_mix(proposal, context, key_temps, preset, duty)?
        }
        LaptopTempStrategy::ThinkpadSensor => {
            build_laptop_graph(proposal, context, key_temps, preset, false, duty)?
        }
        // EmaCpu, or MixCpuGpu with no GPU temp (degrade to the EMA-CPU default).
        _ => build_laptop_graph(proposal, context, key_temps, preset, true, duty)?,
    };
    proposal.assign(assignment, profile_uid);
    Ok(())
}

/// Silent and Balanced idle the fan off below their first knee, so their floor is NOT raised to
/// the channel minimum. Performance has no 0% entry and keeps the clamp so it stays spinning.
fn laptop_holds_floor(preset: Preset) -> bool {
    match preset {
        Preset::Silent | Preset::Balanced => false,
        Preset::Performance => true,
    }
}

/// A laptop fan as a single Graph off the CPU temp, optionally EMA-smoothed.
fn build_laptop_graph(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    preset: Preset,
    smooth: bool,
    duty: ChannelDuty,
) -> Result<ProfileUID, CCError> {
    let cpu = key_temps.cpu.as_ref().ok_or_else(|| CCError::UserError {
        msg: "A laptop fan was assigned but no CPU temp was selected".to_string(),
    })?;
    let entry = TUNING.laptop.get(preset);
    let name = profile_name("Laptop Fan", preset, duty);
    let profile_uid = if smooth {
        build_from_entry(proposal, context, entry, cpu, &name, duty)
    } else {
        build_entry_with_source(proposal, entry, cpu.clone(), &name, duty)
    };
    Ok(profile_uid)
}

/// A laptop fan as a Mix(CPU, GPU) Max, each member a Graph over the preset's source. Used when
/// the user picks the Mix strategy and a GPU temp is available.
fn build_laptop_mix(
    proposal: &mut Proposal,
    context: &DeviceContext,
    key_temps: &KeyTemps,
    preset: Preset,
    duty: ChannelDuty,
) -> Result<ProfileUID, CCError> {
    let cpu = key_temps.cpu.as_ref().ok_or_else(|| CCError::UserError {
        msg: "A laptop fan was assigned but no CPU temp was selected".to_string(),
    })?;
    let gpu = key_temps.gpu.as_ref().ok_or_else(|| CCError::UserError {
        msg: "A laptop Mix source needs a GPU temp".to_string(),
    })?;
    let cpu_member = build_laptop_member(proposal, context, cpu, preset, duty);
    let gpu_member = build_laptop_member(proposal, context, gpu, preset, duty);
    let mix = build_mix_profile(
        &profile_name("Laptop Mix", preset, duty),
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
    duty: ChannelDuty,
) -> ProfileUID {
    let entry = TUNING.laptop.get(preset);
    build_from_entry(
        proposal,
        context,
        entry,
        temp,
        &profile_name(&format!("Laptop {}", temp.temp_name), preset, duty),
        duty,
    )
}

/// Builds a profile from a tuning entry using an already-resolved source. A Fixed entry becomes a
/// Fixed profile (the source is unused); a Graph entry applies its curve and named function to the
/// source. `duty` carries the channel's reading of those duties: its scale, and the floor that
/// keeps a low curve from stalling a fan that needs more to spin. The entry's smoothing is NOT
/// applied here: the caller resolves the source, so kinds that follow a raw signal (radiator,
/// laptop `ThinkPad` sensor) can bypass smoothing.
fn build_entry_with_source(
    proposal: &mut Proposal,
    entry: &SetupEntry,
    source: TempSource,
    name: &str,
    duty: ChannelDuty,
) -> ProfileUID {
    match entry {
        SetupEntry::Fixed { duty: fixed } => {
            proposal.intern_profile(build_fixed_profile(name, duty.apply_to_duty(*fixed)))
        }
        SetupEntry::Graph {
            curve, function, ..
        } => {
            let function_uid = proposal.intern_function(build_function(function));
            let curve = duty.apply_to_curve(curve.clone());
            assert_valid_curve(&curve);
            proposal.intern_profile(build_graph_profile(name, source, function_uid, curve))
        }
    }
}

/// Builds a profile from a tuning entry, resolving its (optionally EMA-smoothed) source from the
/// given base temp per the entry's `smoothing`. Used by the kinds whose smoothing is driven by the
/// tuning data (CPU cooler, GPU fan, case members, laptop EMA, Silent pump).
fn build_from_entry(
    proposal: &mut Proposal,
    context: &DeviceContext,
    entry: &SetupEntry,
    base_temp: &TempSource,
    name: &str,
    duty: ChannelDuty,
) -> ProfileUID {
    let source = match entry {
        SetupEntry::Graph { smoothing, .. } => {
            resolve_entry_source(proposal, context, base_temp, smoothing.as_ref())
        }
        SetupEntry::Fixed { .. } => base_temp.clone(),
    };
    build_entry_with_source(proposal, entry, source, name, duty)
}

/// Resolves the temp source for a graph entry: the raw temp, or (when the entry sets smoothing and
/// a custom-sensors device exists) an EMA custom sensor wrapping it. The EMA sensor is created and
/// de-duplicated here.
fn resolve_entry_source(
    proposal: &mut Proposal,
    context: &DeviceContext,
    base: &TempSource,
    smoothing: Option<&SmoothingSpec>,
) -> TempSource {
    let Some(smoothing) = smoothing else {
        return base.clone();
    };
    let Some(custom_sensors_device_uid) = context.custom_sensors_device_uid.clone() else {
        return base.clone();
    };
    let sensor_id =
        proposal.intern_custom_sensor(build_ema_sensor(base.clone(), smoothing.ema_window_seconds));
    TempSource {
        temp_name: sensor_id,
        device_uid: custom_sensors_device_uid,
    }
}

/// Builds a Function from a named spec in the tuning data. A spec with no Standard fields is a
/// plain (Identity) function; otherwise a Standard hysteresis function. The name is validated at
/// load, so the lookup cannot miss in a shipped build.
fn build_function(name: &str) -> Function {
    let spec = TUNING
        .functions
        .get(name)
        .expect("tuning function name is validated at load");
    let has_standard_fields =
        spec.deviance.is_some() || spec.only_downward.is_some() || spec.response_delay.is_some();
    let kind = if has_standard_fields {
        FunctionKind::Standard {
            deviance: spec.deviance,
            only_downward: spec.only_downward,
            response_delay: spec.response_delay,
        }
    } else {
        FunctionKind::Identity
    };
    // Step sizes are optional in the tuning data: an unset field keeps the daemon default.
    let defaults = Function::default();
    Function {
        uid: Uuid::new_v4().to_string(),
        name: spec.name.clone(),
        step_size_min: spec.step_size_min.unwrap_or(defaults.step_size_min),
        step_size_max: spec.step_size_max.unwrap_or(defaults.step_size_max),
        step_size_min_decreasing: spec
            .step_size_min_decreasing
            .unwrap_or(defaults.step_size_min_decreasing),
        step_size_max_decreasing: spec
            .step_size_max_decreasing
            .unwrap_or(defaults.step_size_max_decreasing),
        threshold_hopping: defaults.threshold_hopping,
        bypass_min_at_extremes: defaults.bypass_min_at_extremes,
        kind,
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
            sources: vec![SensorSource::from_temp(source, 1)],
        },
        children: Vec::new(),
        parents: Vec::new(),
    }
}

/// Asserts a generated curve is well-formed: enough points spanning enough temperature to render
/// as a curve, temps strictly increasing, duties non-decreasing. Every generated curve descends
/// from a validated tuning curve and clamping only raises duties, so these cannot fire in a
/// shipped build; they catch a builder that reshapes a curve on its way through.
fn assert_valid_curve(curve: &[(Temp, Duty)]) {
    debug_assert!(
        curve.len() >= MIN_CURVE_POINTS,
        "curve must have at least {MIN_CURVE_POINTS} points"
    );
    debug_assert!(
        curve.last().map(|point| point.0).unwrap_or_default()
            - curve.first().map(|point| point.0).unwrap_or_default()
            >= MIN_CURVE_TEMP_SPREAD,
        "curve must span at least {MIN_CURVE_TEMP_SPREAD}C"
    );
    debug_assert!(
        curve.windows(2).all(|w| w[0].0 < w[1].0),
        "curve temps must strictly increase"
    );
    debug_assert!(
        curve.windows(2).all(|w| w[0].1 <= w[1].1),
        "curve duties must not decrease"
    );
}

/// A Graph profile following a single temp source. A fresh UID is assigned. The axis range is
/// pinned to the curve's own end points so the UI opens on the curve rather than on a fraction of
/// the device's full temperature range.
fn build_graph_profile(
    name: &str,
    temp_source: TempSource,
    function_uid: FunctionUID,
    speed_profile: Vec<(Temp, Duty)>,
) -> Profile {
    let temp_min = speed_profile.first().map(|point| point.0);
    let temp_max = speed_profile.last().map(|point| point.0);
    debug_assert!(temp_min < temp_max, "axis range must be non-empty");
    Profile {
        uid: Uuid::new_v4().to_string(),
        name: name.to_string(),
        function_uid,
        kind: ProfileKind::Graph {
            speed_profile: Some(speed_profile),
            temp_source: Some(temp_source),
            temp_min,
            temp_max,
        },
    }
}

/// How one channel reads the duties of the profile it is given.
///
/// A calibrated channel reads them as true duty (the fraction of the fan's usable RPM span), so the
/// authored PWM curve is converted before it is written into a profile. Everything else takes them
/// as raw PWM. The two scales never share a profile name, or reuse-by-name across runs would hand
/// a calibrated fan a curve written for a raw one.
#[derive(Debug, Clone, Copy)]
struct ChannelDuty {
    /// Raise every duty to at least this, so a low curve cannot stall a fan that needs more to
    /// spin. None for channels that may idle at 0% (a zero-RPM GPU fan, a quiet laptop preset).
    floor: Option<Duty>,
    /// The channel remaps duties through its calibration.
    calibrated: bool,
}

impl ChannelDuty {
    /// Applies the channel's reading of a curve: convert to its scale, then hold the floor. A
    /// calibrated channel has no dead zone left to clear, so its floor is already 0.
    fn apply_to_curve(self, curve: Vec<(Temp, Duty)>) -> Vec<(Temp, Duty)> {
        curve
            .into_iter()
            .map(|(temp, duty)| (temp, self.apply_to_duty(duty)))
            .collect()
    }

    fn apply_to_duty(self, duty: Duty) -> Duty {
        let scaled = if self.calibrated {
            TUNING.scale.pwm_to_true_duty(duty)
        } else {
            duty
        };
        self.floor.map_or(scaled, |floor| scaled.max(floor))
    }
}

/// The device facts the generator needs: each controllable channel's effective (calibration
/// aware) minimum duty, which channels remap duties through a calibration, and the custom-sensors
/// device UID where generated EMA and Delta sensors live so a profile can reference them.
struct DeviceContext {
    min_duty_by_channel: HashMap<ChannelKey, Duty>,
    calibrated_channels: HashSet<ChannelKey>,
    custom_sensors_device_uid: Option<DeviceUID>,
}

type ChannelKey = (DeviceUID, ChannelName);

impl DeviceContext {
    /// `calibrated_channels` holds the channels that remap duties, which is the Smooth-calibrated
    /// ones: a Stepped or absent calibration is a passthrough and reads duty as raw PWM.
    fn from_devices(devices: &[DeviceDto], calibrated_channels: HashSet<ChannelKey>) -> Self {
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
            calibrated_channels,
            custom_sensors_device_uid,
        }
    }

    /// How the channel reads a profile's duties. `floor` is the caller's call: pass None for a
    /// channel that may idle at 0%, otherwise the channel's effective minimum duty is held.
    fn duty_for(&self, assignment: &FanAssignment, floored: bool) -> ChannelDuty {
        let key = (
            assignment.device_uid.clone(),
            assignment.channel_name.clone(),
        );
        ChannelDuty {
            floor: floored.then(|| self.min_duty_by_channel.get(&key).copied().unwrap_or(0)),
            calibrated: self.calibrated_channels.contains(&key),
        }
    }
}

/// What the system already has, indexed by the names this generator produces. Every generated name
/// is deterministic and encodes its kind and preset ("AIO Radiator (Balanced)", "Auto Fan (Silent)",
/// "Auto EMA Tctl 8s"), so a name match is the same entity from an earlier run and the run reuses
/// it instead of minting a suffixed copy. Matching on the definition instead would miss any entity
/// the user has since edited and duplicate it again.
///
/// Reuse is read-only: an existing entity is never rewritten, so edits survive a re-run. The cost
/// is that new tuning does not reach a profile whose name is already taken until the user deletes
/// it.
struct Existing<'a> {
    profile_uid_by_name: HashMap<&'a str, &'a ProfileUID>,
    profile_uid_by_signature: HashMap<String, &'a ProfileUID>,
    function_uid_by_name: HashMap<&'a str, &'a FunctionUID>,
    function_uid_by_signature: HashMap<String, &'a FunctionUID>,
    custom_sensor_ids: HashSet<&'a str>,
}

impl<'a> Existing<'a> {
    fn from_entities(
        profiles: &'a [Profile],
        functions: &'a [Function],
        custom_sensors: &'a [CustomSensor],
    ) -> Self {
        Self {
            profile_uid_by_name: profiles
                .iter()
                .map(|profile| (profile.name.as_str(), &profile.uid))
                .collect(),
            profile_uid_by_signature: profiles
                .iter()
                .map(|profile| (profile_signature(profile), &profile.uid))
                .collect(),
            function_uid_by_name: functions
                .iter()
                .map(|function| (function.name.as_str(), &function.uid))
                .collect(),
            function_uid_by_signature: functions
                .iter()
                .map(|function| (function_signature(function), &function.uid))
                .collect(),
            custom_sensor_ids: custom_sensors
                .iter()
                .map(|sensor| sensor.id.as_str())
                .collect(),
        }
    }

    /// Nothing exists yet, for tests that propose against an empty system.
    #[cfg(test)]
    fn empty() -> Self {
        Self {
            profile_uid_by_name: HashMap::new(),
            profile_uid_by_signature: HashMap::new(),
            function_uid_by_name: HashMap::new(),
            function_uid_by_signature: HashMap::new(),
            custom_sensor_ids: HashSet::new(),
        }
    }
}

/// Accumulates the entities a generation run proposes, de-duplicating custom sensors,
/// functions, and profiles that share an identical definition so the user's lists are not
/// cluttered with copies. Anything that already exists under the same name is reused rather than
/// proposed, so re-running the wizard does not duplicate an earlier run's output.
struct Proposal<'a> {
    custom_sensors: Vec<CustomSensor>,
    functions: Vec<Function>,
    profiles: Vec<Profile>,
    assignments: Vec<ChannelAssignment>,
    custom_sensor_id_by_signature: HashMap<String, TempName>,
    function_uid_by_signature: HashMap<String, FunctionUID>,
    profile_uid_by_signature: HashMap<String, ProfileUID>,
    existing: &'a Existing<'a>,
}

impl<'a> Proposal<'a> {
    fn with_capacity(fan_count: usize, existing: &'a Existing<'a>) -> Self {
        Self {
            custom_sensors: Vec::new(),
            functions: Vec::with_capacity(fan_count),
            profiles: Vec::with_capacity(fan_count),
            assignments: Vec::with_capacity(fan_count),
            custom_sensor_id_by_signature: HashMap::new(),
            function_uid_by_signature: HashMap::with_capacity(fan_count),
            profile_uid_by_signature: HashMap::with_capacity(fan_count),
            existing,
        }
    }

    /// Returns the id of an existing or already-proposed identical custom sensor, or stores this
    /// one and returns its id. A sensor's id is its name, so an existing id is a reuse.
    fn intern_custom_sensor(&mut self, sensor: CustomSensor) -> TempName {
        if self.existing.custom_sensor_ids.contains(sensor.id.as_str()) {
            return sensor.id;
        }
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

    /// Returns the UID of an existing or already-proposed function, or stores this one and returns
    /// its UID. The passed function's fresh UID is discarded on any hit.
    fn intern_function(&mut self, function: Function) -> FunctionUID {
        let signature = function_signature(&function);
        if let Some(existing_uid) = self
            .existing
            .function_uid_by_name
            .get(function.name.as_str())
            .or_else(|| self.existing.function_uid_by_signature.get(&signature))
        {
            let uid = (*existing_uid).clone();
            self.function_uid_by_signature
                .insert(signature, uid.clone());
            return uid;
        }
        if let Some(existing_uid) = self.function_uid_by_signature.get(&signature) {
            return existing_uid.clone();
        }
        let uid = function.uid.clone();
        self.function_uid_by_signature
            .insert(signature, uid.clone());
        self.functions.push(function);
        uid
    }

    /// Returns the UID of an existing or already-proposed profile, or stores this one and returns
    /// its UID. The passed profile's fresh UID is discarded on any hit. Matching the name catches
    /// an earlier run's profile even if the user has since edited it; matching the definition
    /// catches one an earlier run stored under a different name, which happens because identical
    /// members collapse onto whichever kind was built first. Recording the signature of what would
    /// have been built lets the rest of this run collapse onto the reused entity too.
    fn intern_profile(&mut self, profile: Profile) -> ProfileUID {
        let signature = profile_signature(&profile);
        if let Some(existing_uid) = self
            .existing
            .profile_uid_by_name
            .get(profile.name.as_str())
            .or_else(|| self.existing.profile_uid_by_signature.get(&signature))
        {
            let uid = (*existing_uid).clone();
            self.profile_uid_by_signature.insert(signature, uid.clone());
            return uid;
        }
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
/// only differ by those are treated as duplicates. Combines the shared step-size/safety fields
/// with the `kind` (which carries the type and all type-specific fields) via Debug formatting.
fn function_signature(function: &Function) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{:?}",
        function.step_size_min,
        function.step_size_max,
        function.step_size_min_decreasing,
        function.step_size_max_decreasing,
        function.threshold_hopping,
        function.bypass_min_at_extremes,
        function.kind,
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

    /// A context with a custom-sensors device present, no per-channel minimum duties and no
    /// calibrated channels, so curves reach profiles in the scale they were authored in.
    fn test_context() -> DeviceContext {
        DeviceContext {
            min_duty_by_channel: HashMap::new(),
            calibrated_channels: HashSet::new(),
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
            calibrated_channels: HashSet::new(),
            custom_sensors_device_uid: Some("dev-custom-sensors".to_string()),
        }
    }

    /// A context where one channel is calibrated, so it reads duties on the true-duty scale. A
    /// calibrated channel's effective minimum duty is 0: the mapping absorbs the dead zone.
    fn context_with_calibrated(device_uid: &str, channel_name: &str) -> DeviceContext {
        let mut calibrated_channels = HashSet::new();
        calibrated_channels.insert((device_uid.to_string(), channel_name.to_string()));
        DeviceContext {
            min_duty_by_channel: HashMap::new(),
            calibrated_channels,
            custom_sensors_device_uid: Some("dev-custom-sensors".to_string()),
        }
    }

    /// Goal: a single CPU cooler yields a valid Graph profile plus its function, wired
    /// together and assigned to the fan. Method: generate, then assert the entity counts, the
    /// profile shape, and that the assignment points at the produced profile.
    #[test]
    fn generates_cpu_cooler_profile_and_function() {
        let response = propose(
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
        assert_eq!(function.only_downward(), Some(true));
        assert_eq!(profile.function_uid, function.uid);
        assert_eq!(response.assignments[0].profile_uid, profile.uid);
        assert!(response.assignments[0].replaces_profile_name.is_none());
    }

    /// Goal: two CPU coolers with the same preset and temp source collapse to one shared
    /// profile and function, with both fans assigned to it. Method: generate for two channels
    /// and assert the dedup leaves one profile/function but two assignments to the same UID.
    #[test]
    fn dedups_identical_cpu_coolers() {
        let response = propose(
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
        assert!(propose(&request, &test_context()).is_err());
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
        let response = propose(&request, &test_context()).expect("generates");
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
        let response = propose(
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
        let response = propose(
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
        let response =
            propose(&cpu_cooler_request(&["fan1"], Preset::Silent), &context).expect("generates");
        let curve = response.profiles[0].speed_profile().expect("has curve");
        assert!(curve.iter().all(|(_, duty)| *duty >= 40));
    }

    /// Goal: a channel minimum duty raises the Silent AIO pump curve floor so a high-min_duty pump
    /// cannot stall. Method: generate Silent with `min_duty` 70 and assert every duty is at least 70.
    #[test]
    fn aio_pump_silent_floor_clamped_to_min_duty() {
        let context = context_with_min_duty("dev-aio-1", "fan1", 70);
        let response = propose(
            &aio_request(FanKind::AioPump, Preset::Silent, cpu_only()),
            &context,
        )
        .expect("generates");
        let curve = response.profiles[0].speed_profile().expect("has curve");
        assert!(curve.iter().all(|(_, duty)| *duty >= 70));
    }

    /// Goal: a channel minimum duty raises the AIO radiator curve floor so a high-min_duty fan
    /// cannot stall. Method: generate Silent off a liquid temp with `min_duty` 50 and assert every
    /// duty is at least 50.
    #[test]
    fn aio_radiator_floor_clamped_to_min_duty() {
        let key_temps = KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: None,
            liquid: Some(liquid_temp()),
            ambient: None,
        };
        let context = context_with_min_duty("dev-aio-1", "fan1", 50);
        let response = propose(
            &aio_request(FanKind::AioRadiator, Preset::Silent, key_temps),
            &context,
        )
        .expect("generates");
        let curve = response.profiles[0].speed_profile().expect("has curve");
        assert!(curve.iter().all(|(_, duty)| *duty >= 50));
    }

    /// Goal: the Performance laptop follows the raw temp so it reacts at once. Method: generate
    /// Performance and assert no EMA sensor was created and the source is the raw CPU temp.
    #[test]
    fn laptop_performance_uses_raw_temp() {
        let response = propose(
            &laptop_request(Preset::Performance, None, cpu_only()),
            &test_context(),
        )
        .expect("generates");
        assert!(response.custom_sensors.is_empty());
        assert_eq!(response.profiles[0].temp_source(), Some(&cpu_temp()));
    }

    /// Goal: a channel minimum duty raises the Performance laptop curve floor so a high-min_duty
    /// fan cannot stall. Method: generate Performance with `min_duty` 50 and assert every duty is
    /// at least 50.
    #[test]
    fn laptop_performance_floor_clamped_to_min_duty() {
        let context = context_with_min_duty("dev-laptop-1", "fan1", 50);
        let response = propose(
            &laptop_request(Preset::Performance, None, cpu_only()),
            &context,
        )
        .expect("generates");
        let curve = response.profiles[0].speed_profile().expect("has curve");
        assert!(curve.iter().all(|(_, duty)| *duty >= 50));
    }

    /// Goal: the quiet laptop presets idle the fan off below their knee even when the channel
    /// reports a non-zero minimum duty. Method: generate each with `min_duty` 50 and assert the
    /// curve still has a 0% point.
    #[test]
    fn laptop_quiet_presets_keep_zero_idle() {
        for preset in [Preset::Silent, Preset::Balanced] {
            let context = context_with_min_duty("dev-laptop-1", "fan1", 50);
            let response =
                propose(&laptop_request(preset, None, cpu_only()), &context).expect("generates");
            let curve = response.profiles[0].speed_profile().expect("has curve");
            assert!(
                curve.iter().any(|(_, duty)| *duty == 0),
                "{preset} idles the fan off"
            );
        }
    }

    /// Goal: each member of a laptop Mix follows the preset's floor rule, so a quiet preset can
    /// idle off while Performance cannot stall. Method: generate the Mix strategy with a GPU temp
    /// and `min_duty` 50 for both a quiet preset and Performance, then assert each member's floor.
    #[test]
    fn laptop_mix_members_follow_preset_floor() {
        let key_temps = || KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: Some(gpu_temp()),
            liquid: None,
            ambient: None,
        };
        let member_curves = |preset| {
            let context = context_with_min_duty("dev-laptop-1", "fan1", 50);
            let response = propose(
                &laptop_request(preset, Some(LaptopTempStrategy::MixCpuGpu), key_temps()),
                &context,
            )
            .expect("generates");
            response
                .profiles
                .iter()
                .filter_map(|p| p.speed_profile().cloned())
                .collect::<Vec<_>>()
        };

        let balanced = member_curves(Preset::Balanced);
        assert_eq!(balanced.len(), 2, "two mix members");
        for curve in balanced {
            assert!(curve.iter().any(|(_, duty)| *duty == 0));
        }

        let performance = member_curves(Preset::Performance);
        assert_eq!(performance.len(), 2, "two mix members");
        for curve in performance {
            assert!(curve.iter().all(|(_, duty)| *duty >= 50));
        }
    }

    /// Goal: GPU fans keep a 0% idle even when the channel reports a non-zero minimum duty, to
    /// preserve zero-RPM. Method: generate with `min_duty` 30 and assert the curve still has a 0%
    /// point.
    #[test]
    fn gpu_fan_preserves_zero_rpm_idle() {
        let context = context_with_min_duty("dev-mb-1", "fan1", 30);
        let response =
            propose(&gpu_request(&["fan1"], Preset::Silent), &context).expect("generates");
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
        assert!(propose(&request, &test_context()).is_err());
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

    /// Proposes against an empty system: nothing exists yet, so nothing is reused.
    fn propose(
        request: &GenerateProfilesRequest,
        context: &DeviceContext,
    ) -> Result<GenerateProfilesResponse, CCError> {
        generate_proposal(request, context, &Existing::empty())
    }

    fn count_p_type(profiles: &[Profile], p_type: &ProfileType) -> usize {
        profiles.iter().filter(|p| &p.p_type() == p_type).count()
    }

    fn find_profile<'a>(profiles: &'a [Profile], uid: &str) -> &'a Profile {
        profiles
            .iter()
            .find(|profile| profile.uid == uid)
            .expect("proposed profile exists")
    }

    /// The curve of a graph tuning entry, for asserting generated output against the data.
    fn entry_curve(entry: &SetupEntry) -> &[(Temp, Duty)] {
        match entry {
            SetupEntry::Graph { curve, .. } => curve,
            SetupEntry::Fixed { .. } => panic!("expected a graph entry"),
        }
    }

    /// Goal: with a GPU temp, case fans produce two member graphs, one Max Mix base and a single
    /// Overlay, since only exhaust needs one. Method: generate and assert the per-type profile
    /// counts and the assignment count.
    #[test]
    fn case_fans_build_mix_base_and_one_exhaust_overlay() {
        let response =
            propose(&case_request(Preset::Balanced, true), &test_context()).expect("generates");
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Graph), 2);
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Mix), 1);
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Overlay), 1);
        assert_eq!(response.assignments.len(), 2);
    }

    /// Goal: intake is given the shared base as it stands, and exhaust overlays that same base
    /// with a negative offset for positive pressure. Method: generate, then assert the intake fan
    /// is assigned the Mix itself while the exhaust Overlay references it and carries the bias.
    #[test]
    fn case_intake_takes_the_base_and_exhaust_overlays_it() {
        let response =
            propose(&case_request(Preset::Balanced, true), &test_context()).expect("generates");
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
        let offsets = exhaust.offset_profile().expect("has an offset");
        assert_eq!(
            offsets.len(),
            1,
            "a static offset, not a curve the user has to decode"
        );
        assert_eq!(
            offsets[0].1,
            -Offset::try_from(TUNING.case.pressure.exhaust_bias_percent).expect("fits"),
            "exhaust runs the configured bias below the base for positive pressure"
        );
        let intake = response
            .assignments
            .iter()
            .find(|assignment| assignment.channel_name == "fan2")
            .expect("the intake fan is assigned");
        assert_eq!(
            intake.profile_uid, mix.uid,
            "intake follows the airflow demand itself, with nothing wrapped around it"
        );
    }

    /// Goal: without a GPU temp, the base degrades to a single CPU graph (no Mix), still with one
    /// overlays referencing it. Method: generate without a GPU temp and assert the shape.
    #[test]
    fn case_fans_degrade_to_cpu_graph_without_gpu() {
        let response =
            propose(&case_request(Preset::Balanced, false), &test_context()).expect("generates");
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
        assert_eq!(count_p_type(&response.profiles, &ProfileType::Overlay), 1);
        let graph = response
            .profiles
            .iter()
            .find(|p| p.p_type() == ProfileType::Graph)
            .expect("has the base graph");
        let exhaust = response
            .profiles
            .iter()
            .find(|p| p.p_type() == ProfileType::Overlay)
            .expect("has the exhaust overlay");
        assert_eq!(
            exhaust.member_profile_uids(),
            [graph.uid.clone()].as_slice()
        );
        for assignment in &response.assignments {
            let uid = &assignment.profile_uid;
            assert!(
                *uid == graph.uid || *uid == exhaust.uid,
                "both fans resolve to the one base"
            );
        }
    }

    /// Goal: case fans need a CPU temp. Method: omit the CPU temp and assert generation errors.
    #[test]
    fn case_fans_without_cpu_temp_errors() {
        let mut request = case_request(Preset::Balanced, true);
        request.key_temps.cpu = None;
        assert!(propose(&request, &test_context()).is_err());
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
        assert!(propose(&request, &test_context()).is_err());
    }

    /// Goal: Silent case fans smooth both members, creating one EMA sensor per temp. Method:
    /// generate Silent with CPU and GPU temps and assert two EMA sensors.
    #[test]
    fn case_fans_silent_create_ema_per_member() {
        let response =
            propose(&case_request(Preset::Silent, true), &test_context()).expect("generates");
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
            let response = propose(
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

    /// Goal: the Silent pump is a graph off a 50% floor, smoothed off the CPU temp. Method:
    /// generate and assert the curve points and that an EMA sensor was created.
    #[test]
    fn aio_pump_silent_is_a_smoothed_graph() {
        let response = propose(
            &aio_request(FanKind::AioPump, Preset::Silent, cpu_only()),
            &test_context(),
        )
        .expect("generates");
        let pump = &response.profiles[0];
        assert_eq!(pump.p_type(), ProfileType::Graph);
        assert_eq!(
            pump.speed_profile().expect("has curve").as_slice(),
            entry_curve(TUNING.aio_pump.get(Preset::Silent))
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
        let response = propose(
            &aio_request(FanKind::AioPump, Preset::Silent, key_temps),
            &test_context(),
        )
        .expect("generates");
        assert_eq!(response.custom_sensors.len(), 1);
        assert!(response.custom_sensors[0]
            .sources()
            .iter()
            .any(|s| s.temp_source() == liquid_temp()));
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
        assert!(propose(
            &aio_request(FanKind::AioPump, Preset::Silent, empty),
            &test_context()
        )
        .is_err());
    }

    /// Goal: with a liquid temp but no ambient, the radiator follows the raw liquid band with no
    /// Delta sensor. Method: generate and assert the source and curve.
    #[test]
    fn aio_radiator_off_liquid_band() {
        let key_temps = KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: None,
            liquid: Some(liquid_temp()),
            ambient: None,
        };
        let response = propose(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, key_temps),
            &test_context(),
        )
        .expect("generates");
        let radiator = &response.profiles[0];
        assert_eq!(radiator.temp_source(), Some(&liquid_temp()));
        assert_eq!(
            radiator.speed_profile().expect("has curve").as_slice(),
            entry_curve(TUNING.aio_radiator.liquid.get(Preset::Balanced))
        );
        assert!(response.custom_sensors.is_empty());
    }

    /// Goal: with both liquid and ambient temps, the radiator follows an auto-created Delta sensor.
    /// Method: generate and assert the Delta sensor, source, and curve.
    #[test]
    fn aio_radiator_off_delta_creates_sensor() {
        let key_temps = KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: None,
            liquid: Some(liquid_temp()),
            ambient: Some(ambient_temp()),
        };
        let response = propose(
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
            entry_curve(TUNING.aio_radiator.delta.get(Preset::Balanced))
        );
    }

    /// Goal: with only a CPU temp, the radiator falls back to the CPU-cooler curve. Method:
    /// generate and assert the source is the CPU temp and the curve matches the CPU band.
    #[test]
    fn aio_radiator_falls_back_to_cpu() {
        let response = propose(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, cpu_only()),
            &test_context(),
        )
        .expect("generates");
        let radiator = &response.profiles[0];
        assert_eq!(radiator.temp_source(), Some(&cpu_temp()));
        // The CPU fallback reuses the cpu_cooler curve from the tuning data.
        let SetupEntry::Graph { curve, .. } = TUNING.cpu_cooler.get(Preset::Balanced) else {
            panic!("cpu_cooler entry should be a graph");
        };
        assert_eq!(radiator.speed_profile().unwrap(), curve);
    }

    /// Goal: with a GPU temp the radiator becomes a Mix(loop, GPU) Max, so the card's heat reaches
    /// the fans that move the case air carrying it. Method: generate with liquid and GPU temps and
    /// assert the assigned profile is the Mix and its two members are the liquid and GPU graphs.
    #[test]
    fn aio_radiator_mixes_gpu_when_present() {
        let key_temps = KeyTemps {
            cpu: None,
            gpu: Some(gpu_temp()),
            liquid: Some(liquid_temp()),
            ambient: None,
        };
        let response = propose(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, key_temps),
            &test_context(),
        )
        .expect("generates");
        let assigned_uid = &response.assignments[0].profile_uid;
        let mix = find_profile(&response.profiles, assigned_uid);
        assert_eq!(mix.p_type(), ProfileType::Mix);
        assert_eq!(mix.mix_function_type(), Some(ProfileMixFunctionType::Max));
        assert_eq!(mix.member_profile_uids().len(), 2);
        let loop_member = find_profile(&response.profiles, &mix.member_profile_uids()[0]);
        assert_eq!(loop_member.name, "AIO Radiator Liquid (Balanced)");
        assert_eq!(loop_member.temp_source(), Some(&liquid_temp()));
        let gpu_member = find_profile(&response.profiles, &mix.member_profile_uids()[1]);
        assert_eq!(gpu_member.name, "AIO Radiator GPU (Balanced)");
        assert_eq!(gpu_member.temp_source(), Some(&gpu_temp()));
        assert_eq!(
            gpu_member.speed_profile().expect("has curve").as_slice(),
            entry_curve(TUNING.gpu_fan.get(Preset::Balanced)),
            "the GPU member reuses the GPU fan curve"
        );
    }

    /// Goal: in the Mix only the loop member carries the channel's minimum-duty floor, so Max can
    /// never fall below it, while the unclamped GPU member stays low enough that an idle card does
    /// not lift the fan. Method: generate with a 50% minimum and assert the loop member is clamped
    /// and the GPU member opens no higher than the loop member does.
    #[test]
    fn aio_radiator_mix_clamps_loop_member_only() {
        let key_temps = KeyTemps {
            cpu: None,
            gpu: Some(gpu_temp()),
            liquid: Some(liquid_temp()),
            ambient: None,
        };
        let response = propose(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, key_temps),
            &context_with_min_duty("dev-aio-1", "fan1", 50),
        )
        .expect("generates");
        let mix = find_profile(&response.profiles, &response.assignments[0].profile_uid);
        let loop_member = find_profile(&response.profiles, &mix.member_profile_uids()[0]);
        assert!(
            loop_member
                .speed_profile()
                .expect("has curve")
                .iter()
                .all(|(_, duty)| *duty >= 50),
            "the loop member holds the floor for the Max"
        );
        let gpu_member = find_profile(&response.profiles, &mix.member_profile_uids()[1]);
        assert!(
            gpu_member.speed_profile().expect("has curve")[0].1
                <= loop_member.speed_profile().expect("has curve")[0].1,
            "an idle GPU must not lift the fan"
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
        assert!(propose(
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
        let response = propose(
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
        assert_eq!(response.functions[0].only_downward(), Some(true));
    }

    /// Goal: the ThinkPad-sensor strategy follows the raw CPU temp with no EMA sensor. Method:
    /// generate with that strategy and assert no custom sensors and the raw CPU source.
    #[test]
    fn laptop_thinkpad_sensor_uses_raw_cpu() {
        let response = propose(
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
        let response = propose(
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
        let SetupEntry::Graph {
            smoothing: Some(desktop_silent),
            ..
        } = TUNING.cpu_cooler.get(Preset::Silent)
        else {
            panic!("cpu_cooler Silent should smooth");
        };
        assert!(
            time_window_seconds > desktop_silent.ema_window_seconds,
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
        let response = propose(
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
        let response = propose(
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
        assert!(propose(
            &laptop_request(Preset::Balanced, None, empty),
            &test_context()
        )
        .is_err());
    }

    /// Goal: a calibrated channel gets the curve converted to the scale it actually reads, so the
    /// same preset means the same fan speed calibrated or not. Method: generate a CPU cooler on a
    /// calibrated channel and assert every duty matches the nominal conversion of the authored
    /// curve, and that the temps are untouched.
    #[test]
    fn a_calibrated_channel_gets_the_curve_converted() {
        let response = propose(
            &cpu_cooler_request(&["fan1"], Preset::Balanced),
            &context_with_calibrated("dev-mb-1", "fan1"),
        )
        .expect("generates");
        let authored = entry_curve(TUNING.cpu_cooler.get(Preset::Balanced));
        let curve = response.profiles[0].speed_profile().expect("has curve");
        assert_eq!(curve.len(), authored.len());
        for (converted, (temp, duty)) in curve.iter().zip(authored) {
            assert_eq!(converted.0, *temp, "temps are not rescaled");
            assert_eq!(converted.1, TUNING.scale.pwm_to_true_duty(*duty));
            assert!(converted.1 <= *duty, "the true scale never asks for more");
        }
        assert!(
            curve[0].1 < authored[0].1,
            "the true scale drops the dead zone below full speed"
        );
        assert_eq!(
            curve[curve.len() - 1].1,
            authored[authored.len() - 1].1,
            "full speed is full speed on either scale"
        );
    }

    /// Goal: the two scales never share a profile name, or a later run's reuse-by-name would hand
    /// a calibrated fan a curve written for a raw one. Method: generate the same kind and preset
    /// on a calibrated and an uncalibrated channel and assert the names and curves differ.
    #[test]
    fn the_calibrated_scale_gets_its_own_profile() {
        let raw = propose(
            &cpu_cooler_request(&["fan1"], Preset::Balanced),
            &test_context(),
        )
        .expect("generates");
        let calibrated = propose(
            &cpu_cooler_request(&["fan1"], Preset::Balanced),
            &context_with_calibrated("dev-mb-1", "fan1"),
        )
        .expect("generates");
        assert_eq!(raw.profiles[0].name, "CPU Cooler (Balanced)");
        assert_eq!(
            calibrated.profiles[0].name,
            "CPU Cooler (Balanced, Calibrated)"
        );
        assert_ne!(
            raw.profiles[0].speed_profile(),
            calibrated.profiles[0].speed_profile()
        );
    }

    /// Goal: converting keeps a curve well formed, so the shape rules hold on both scales. Method:
    /// generate every kind on a calibrated channel and assert each curve still climbs, keeps its
    /// zero-idle entries at zero, and never lands on an unspinnable 0 where the author asked for
    /// movement.
    #[test]
    fn converting_keeps_curves_well_formed() {
        let mut request = full_request();
        let mut calibrated_channels = HashSet::new();
        for assignment in &request.assignments {
            calibrated_channels.insert((
                assignment.device_uid.clone(),
                assignment.channel_name.clone(),
            ));
        }
        request
            .assignments
            .retain(|assignment| assignment.kind != FanKind::LaptopFan);
        let context = DeviceContext {
            min_duty_by_channel: HashMap::new(),
            calibrated_channels,
            custom_sensors_device_uid: Some("dev-custom-sensors".to_string()),
        };
        let response = propose(&request, &context).expect("generates");
        let mut checked = 0_usize;
        for profile in &response.profiles {
            let Some(curve) = profile.speed_profile() else {
                continue;
            };
            assert!(
                curve.windows(2).all(|w| w[0].1 <= w[1].1),
                "{} duties still climb",
                profile.name
            );
            assert!(
                curve.iter().all(|(_, duty)| *duty <= 100),
                "{} stays within range",
                profile.name
            );
            checked += 1;
        }
        assert!(checked > 0, "the full config proposes graphs");
    }

    /// Goal: a curve that idles the fan off keeps idling it off after conversion, since a
    /// zero-RPM GPU fan must stay stopped. Method: generate a GPU fan on a calibrated channel and
    /// assert its opening duty is still 0.
    #[test]
    fn converting_keeps_a_zero_idle_at_zero() {
        let response = propose(
            &gpu_request(&["fan1"], Preset::Balanced),
            &context_with_calibrated("dev-mb-1", "fan1"),
        )
        .expect("generates");
        let curve = response.profiles[0].speed_profile().expect("has curve");
        assert_eq!(curve[0].1, 0, "zero-RPM idle survives the conversion");
    }

    /// Goal: re-running the wizard reuses what the first run created instead of minting a suffixed
    /// copy of every profile, function and sensor. Method: propose once against an empty system,
    /// feed that output back as the existing entities, propose again, and assert the second run
    /// creates nothing and assigns the same profiles.
    #[test]
    fn rerunning_reuses_the_first_runs_entities() {
        let request = full_request();
        let first = propose(&request, &test_context()).expect("generates");
        let existing =
            Existing::from_entities(&first.profiles, &first.functions, &first.custom_sensors);
        let second =
            generate_proposal(&request, &test_context(), &existing).expect("generates again");

        assert!(
            second.profiles.is_empty(),
            "no duplicate profiles, got {:?}",
            second
                .profiles
                .iter()
                .map(|profile| profile.name.as_str())
                .collect::<Vec<_>>()
        );
        assert!(second.functions.is_empty(), "no duplicate functions");
        assert!(second.custom_sensors.is_empty(), "no duplicate sensors");
        assert_eq!(second.assignments.len(), first.assignments.len());
        for (before, after) in first.assignments.iter().zip(&second.assignments) {
            assert_eq!(
                before.profile_uid, after.profile_uid,
                "{} keeps its profile",
                after.channel_name
            );
        }
    }

    /// Goal: reuse is by name and read-only, so a profile the user edited after generating it is
    /// reused as it stands rather than duplicated or rewritten. Method: propose, edit a proposed
    /// profile's curve, feed it back as existing, and assert the re-run creates nothing and points
    /// at the edited profile.
    #[test]
    fn reuse_keeps_a_user_edited_profile() {
        let request = cpu_cooler_request(&["fan1"], Preset::Balanced);
        let mut first = propose(&request, &test_context()).expect("generates");
        let edited_uid = first.profiles[0].uid.clone();
        let ProfileKind::Graph { speed_profile, .. } = &mut first.profiles[0].kind else {
            panic!("a CPU cooler is a graph");
        };
        *speed_profile = Some(vec![(20.0, 10), (50.0, 50), (80.0, 90)]);

        let existing =
            Existing::from_entities(&first.profiles, &first.functions, &first.custom_sensors);
        let second = generate_proposal(&request, &test_context(), &existing).expect("generates");

        assert!(second.profiles.is_empty(), "the edited profile is reused");
        assert_eq!(second.assignments[0].profile_uid, edited_uid);
    }

    /// Goal: a fan added in a later run joins the profiles the earlier run created, so a
    /// single-fan run behaves like part of the whole-system one. Method: propose for one fan, feed
    /// it back, propose for a second fan of the same kind and preset, and assert it is assigned the
    /// first fan's profile with nothing new created.
    #[test]
    fn a_later_fan_joins_the_existing_profile() {
        let first = propose(
            &cpu_cooler_request(&["fan1"], Preset::Balanced),
            &test_context(),
        )
        .expect("generates");
        let existing =
            Existing::from_entities(&first.profiles, &first.functions, &first.custom_sensors);
        let second = generate_proposal(
            &cpu_cooler_request(&["fan2"], Preset::Balanced),
            &test_context(),
            &existing,
        )
        .expect("generates");

        assert!(second.profiles.is_empty());
        assert_eq!(
            second.assignments[0].profile_uid, first.assignments[0].profile_uid,
            "both fans share one profile"
        );
    }

    /// Goal: the single-fan wizard run creates nothing after a whole-system run, which is the
    /// reason reuse matches definitions and not only names: the full run stored the radiator's GPU
    /// member under the case fan's name, because identical members collapse onto whichever kind was
    /// built first. Method: propose the full system, feed it back, then propose the radiator fan
    /// alone and assert nothing new is proposed.
    #[test]
    fn a_single_fan_rerun_after_a_full_run_creates_nothing() {
        let full = propose(&full_request(), &test_context()).expect("generates");
        let existing =
            Existing::from_entities(&full.profiles, &full.functions, &full.custom_sensors);
        let key_temps = KeyTemps {
            cpu: Some(cpu_temp()),
            gpu: Some(gpu_temp()),
            liquid: Some(liquid_temp()),
            ambient: Some(ambient_temp()),
        };
        let single = generate_proposal(
            &aio_request(FanKind::AioRadiator, Preset::Balanced, key_temps),
            &test_context(),
            &existing,
        )
        .expect("generates");

        assert!(
            single.profiles.is_empty(),
            "no duplicate profiles, got {:?}",
            single
                .profiles
                .iter()
                .map(|profile| profile.name.as_str())
                .collect::<Vec<_>>()
        );
        assert!(single.functions.is_empty(), "no duplicate functions");
        assert!(single.custom_sensors.is_empty(), "no duplicate sensors");
        assert_eq!(single.assignments.len(), 1, "the fan is still assigned");
    }

    /// Goal: every generated graph opens on its own curve, so the UI does not draw a short ramp
    /// across a mostly empty chart. Method: generate a full config and assert each graph's axis
    /// range equals its curve's end points and spans the minimum the tuning rules guarantee.
    #[test]
    fn graph_profiles_pin_axis_to_their_curve() {
        let response = propose(&full_request(), &test_context()).expect("generates");
        let graphs = response
            .profiles
            .iter()
            .filter(|profile| profile.p_type() == ProfileType::Graph);
        let mut checked = 0_usize;
        for profile in graphs {
            let ProfileKind::Graph {
                temp_min, temp_max, ..
            } = &profile.kind
            else {
                panic!("filtered to graphs");
            };
            let curve = profile.speed_profile().expect("a graph has a curve");
            assert_eq!(*temp_min, Some(curve[0].0), "{} axis min", profile.name);
            assert_eq!(
                *temp_max,
                Some(curve[curve.len() - 1].0),
                "{} axis max",
                profile.name
            );
            assert!(curve.len() >= MIN_CURVE_POINTS, "{} points", profile.name);
            assert!(
                curve[curve.len() - 1].0 - curve[0].0 >= MIN_CURVE_TEMP_SPREAD,
                "{} spread",
                profile.name
            );
            checked += 1;
        }
        assert!(checked > 0, "the full config proposes graphs");
    }

    /// Every fan kind at the global Balanced preset with all four key temps available.
    fn full_request() -> GenerateProfilesRequest {
        let all_kinds = [
            ("dev-mb-1", "fan1", FanKind::CpuCooler),
            ("dev-mb-1", "fan2", FanKind::GpuFan),
            ("dev-mb-1", "fan3", FanKind::CaseIntake),
            ("dev-mb-1", "fan4", FanKind::CaseExhaust),
            ("dev-aio-1", "pump", FanKind::AioPump),
            ("dev-aio-1", "rad", FanKind::AioRadiator),
            ("dev-laptop-1", "fan1", FanKind::LaptopFan),
        ];
        GenerateProfilesRequest {
            assignments: all_kinds
                .iter()
                .map(|(device_uid, channel_name, kind)| FanAssignment {
                    device_uid: (*device_uid).to_string(),
                    channel_name: (*channel_name).to_string(),
                    kind: *kind,
                    position: None,
                    laptop_temp_strategy: None,
                })
                .collect(),
            key_temps: KeyTemps {
                cpu: Some(cpu_temp()),
                gpu: Some(gpu_temp()),
                liquid: Some(liquid_temp()),
                ambient: Some(ambient_temp()),
            },
            global_preset: Preset::Balanced,
            preset_overrides: Vec::new(),
        }
    }

    /// Goal: a full config covering all seven kinds generates a coherent proposal, and a function
    /// shared across kinds is de-duplicated rather than copied. Method: build one request with every
    /// kind at the global Balanced preset, then assert every fan is assigned to an existing profile,
    /// every profile is named, and the shared Balanced fan function appears exactly once.
    #[test]
    fn full_config_generates_and_dedups_shared_function() {
        let request = full_request();
        let fan_count = request.assignments.len();
        let response = propose(&request, &test_context()).expect("generates");

        assert_eq!(
            response.assignments.len(),
            fan_count,
            "every fan is assigned"
        );
        let profile_uids: HashMap<ProfileUID, ()> = response
            .profiles
            .iter()
            .map(|p| (p.uid.clone(), ()))
            .collect();
        for assignment in &response.assignments {
            assert!(
                profile_uids.contains_key(&assignment.profile_uid),
                "assignment references an existing profile"
            );
        }
        assert!(
            response.profiles.iter().all(|p| p.name.is_empty().not()),
            "every profile is named"
        );
        let shared_fan_functions = response
            .functions
            .iter()
            .filter(|f| f.name == "Auto Fan (Balanced)")
            .count();
        assert_eq!(
            shared_fan_functions, 1,
            "the Balanced fan function is shared across kinds, not duplicated"
        );
    }
}
