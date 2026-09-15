// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { UID } from '@/models/Device.ts'
import { AllDeviceSettings } from '@/models/UISettings.ts'
import { Type } from 'class-transformer'

export enum HealthEntityType {
    CustomSensor = 'CustomSensor',
    Profile = 'Profile',
    Lcd = 'Lcd',
}

export enum FailsafeKind {
    Temp = 'Temp',
    Channel = 'Channel',
}

export enum HealthState {
    Detected = 'Detected',
    Resolved = 'Resolved',
}

/**
 * Why a fan channel can or cannot be driven, as determined by the daemon from
 * evidence it gathered on this machine.
 *
 * The app states the observation; docs.coolercontrol.org states the remedy.
 * Never name a specific kernel module here.
 */
export enum ChannelVerdict {
    Controllable = 'controllable',
    FirmwareOverride = 'firmware_override',
    FamilyMayNeedOutOfTree = 'family_may_need_out_of_tree',
    NotSupportedByDriver = 'not_supported_by_driver',
    NoPwm = 'no_pwm',
    PwmReadOnly = 'pwm_read_only',
    IgnoresDuty = 'ignores_duty',
    Unverifiable = 'unverifiable',
}

/** The raw sysfs facts a verdict was computed from. */
export class ChannelEvidence {
    has_pwm: boolean = false
    pwm_writable: boolean = false
    has_rpm: boolean = false
    /**
     * A missing pwmN_enable is not a defect. It usually means only that fan
     * control cannot be handed back to the BIOS.
     */
    has_pwm_enable: boolean = false
}

/** A verdict bound to the channel it describes. */
export class ChannelVerdictRef {
    device_uid: UID = ''
    channel_name: string = ''
    verdict: ChannelVerdict = ChannelVerdict.Controllable
    /**
     * Absent when the reporting repository has no sysfs-level facts. Only the
     * hwmon repository can measure these; the others report a cause with no
     * evidence rather than sending zeroed fields that read as measurements.
     */
    @Type(() => ChannelEvidence)
    evidence?: ChannelEvidence
}

/** Machine-scope findings, for hardware that produced no channel at all. */
export enum SystemFindingKind {
    NoDriverBound = 'no_driver_bound',
    Blacklisted = 'blacklisted',
    BlockedByEnvironment = 'blocked_by_environment',
    DetectionUnsupported = 'detection_unsupported',
}

export enum EnvironmentBlocker {
    SecureBoot = 'secure_boot',
    Container = 'container',
    NoDevPort = 'no_dev_port',
}

/** Serialized by the daemon as an internally tagged enum on `kind`. */
export class SystemFinding {
    kind: SystemFindingKind = SystemFindingKind.DetectionUnsupported
    chip_name?: string
    expected_driver?: string
    driver?: string
    reason?: EnvironmentBlocker
}

export class TempSource {
    temp_name: string = ''
    device_uid: UID = ''
}

/**
 * A config entity's temp-source reference, as tracked by the health registries.
 */
export class SourceRef {
    entity_type: HealthEntityType = HealthEntityType.CustomSensor
    /** Profile uid, Custom Sensor id, or the owning device uid for an LCD setting. */
    entity_uid: UID = ''
    entity_name: string = ''
    /** Only set for LCD references. */
    channel_name?: string
    @Type(() => TempSource)
    source: TempSource = new TempSource()
    /** Daemon-resolved name of the device owning the referenced temp, when known. */
    source_device_name?: string
}

/**
 * A device whose driver has stopped answering, so the daemon can neither read from it nor write
 * to it.
 *
 * Deliberately distinct from FailsafeRef. Failsafe means the device is alive, its readings are
 * stale, and safe values are being substituted. Unreachable means the device is not answering at
 * all, so no value can be written to it. Showing the second as the first would tell the user
 * their fans are on a safe curve when in fact no curve can be applied.
 */
export class UnreachableRef {
    device_uid: UID = ''
    device_name: string = ''
    /** Consecutive operations that timed out before the daemon gave up on the device. */
    consecutive_timeouts: number = 0
}

/**
 * A present channel/temp currently serving failsafe values.
 */
export class FailsafeRef {
    device_uid: UID = ''
    name: string = ''
    kind: FailsafeKind = FailsafeKind.Temp
    /** Why the node entered failsafe, as logged by the daemon (not localized). */
    reason: string = ''
}

// The daemon flattens the reference into its SSE delta, so a delta IS a ref plus state.
export class SourceDelta extends SourceRef {
    state: HealthState = HealthState.Detected
}

export class FailsafeDelta extends FailsafeRef {
    state: HealthState = HealthState.Detected
}

export class UnreachableDelta extends UnreachableRef {
    state: HealthState = HealthState.Detected
}

/**
 * Full snapshot from GET /devices/health.
 *
 * Two sections with different meanings. `failsafe`, `missing`, `stale_source`
 * and `firmware_overrides` are current state: conditions that can clear on
 * their own. `channel_capabilities` and `system_findings` are permanent facts
 * about what this hardware can do. Do not present them as the same thing: a
 * capability shown as a fault implies the user should wait for it to resolve.
 */
export class DeviceHealthDTO {
    @Type(() => FailsafeRef)
    failsafe: Array<FailsafeRef> = []
    @Type(() => UnreachableRef)
    unreachable: Array<UnreachableRef> = []
    @Type(() => SourceRef)
    missing: Array<SourceRef> = []
    @Type(() => SourceRef)
    stale_source: Array<SourceRef> = []
    @Type(() => ChannelVerdictRef)
    firmware_overrides: Array<ChannelVerdictRef> = []
    @Type(() => ChannelVerdictRef)
    channel_capabilities: Array<ChannelVerdictRef> = []
    @Type(() => SystemFinding)
    system_findings: Array<SystemFinding> = []
}

/**
 * Docs anchors for each verdict. The app never names a module or a fix; it
 * points at documentation the maintainers can correct without a release.
 */
const DOCS_BASE = 'https://docs.coolercontrol.org/hardware-support.html'

export function verdictDocsLink(verdict: ChannelVerdict): string | undefined {
    switch (verdict) {
        case ChannelVerdict.FamilyMayNeedOutOfTree:
        case ChannelVerdict.FirmwareOverride:
        case ChannelVerdict.IgnoresDuty:
            return `${DOCS_BASE}#motherboard-fans`
        case ChannelVerdict.NotSupportedByDriver:
            return DOCS_BASE
        case ChannelVerdict.NoPwm:
        case ChannelVerdict.PwmReadOnly:
            return `${DOCS_BASE}#laptops`
        case ChannelVerdict.Controllable:
        case ChannelVerdict.Unverifiable:
            return undefined
    }
}

/** One Super-I/O chip the startup probe found. */
export class DetectedChip {
    name: string = ''
    driver: string = ''
    address: string = ''
    base_address: string = ''
    device_id: string = ''
    features: Array<string> = []
    /** Free-form from the daemon: loaded, already_loaded, blacklisted, failed. */
    module_status: string = ''
}

export class DetectEnvironment {
    is_container: boolean = false
    is_secure_boot: boolean = false
    has_dev_port: boolean = false
}

/**
 * What the startup probe found. `probed` false means no probe was made at all,
 * so an empty chip list is "not looked for" rather than "none present".
 */
export class DetectionDTO {
    @Type(() => DetectedChip)
    detected_chips: Array<DetectedChip> = []
    blacklisted: Array<string> = []
    @Type(() => DetectEnvironment)
    environment: DetectEnvironment = new DetectEnvironment()
    probed: boolean = false
}

/** The hardware support page itself, for machine-scope findings. */
export const HARDWARE_SUPPORT_DOCS = DOCS_BASE

/** Where a user with working-but-uncovered hardware should go. */
export const FOUND_SOMETHING_THAT_WORKS = `${DOCS_BASE}#found-something-that-works`

/**
 * Display name for a referenced temp: user-set UI names first, then the
 * daemon-resolved device name, which covers devices that are gone.
 */
export function sourceTempDisplayName(ref: SourceRef, allSettings: AllDeviceSettings): string {
    const sourceSettings = allSettings.get(ref.source.device_uid)
    const tempLabel =
        sourceSettings?.sensorsAndChannels.get(ref.source.temp_name)?.name || ref.source.temp_name
    const deviceName = sourceSettings?.name || ref.source_device_name
    return deviceName ? `${deviceName} | ${tempLabel}` : tempLabel
}

export function sourceKey(ref: SourceRef): string {
    return `${ref.entity_type}/${ref.entity_uid}/${ref.channel_name ?? ''}/${ref.source.device_uid}/${ref.source.temp_name}`
}

export function failsafeKey(ref: FailsafeRef): string {
    return `${ref.device_uid}/${ref.kind}/${ref.name}`
}
