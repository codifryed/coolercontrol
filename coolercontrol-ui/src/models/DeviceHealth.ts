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

import { UID } from '@/models/Device.ts'
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

/** Full snapshot from GET /devices/health. */
export class DeviceHealthDTO {
    @Type(() => FailsafeRef)
    failsafe: Array<FailsafeRef> = []
    @Type(() => SourceRef)
    missing: Array<SourceRef> = []
}

export function sourceKey(ref: SourceRef): string {
    return `${ref.entity_type}/${ref.entity_uid}/${ref.channel_name ?? ''}/${ref.source.device_uid}/${ref.source.temp_name}`
}

export function failsafeKey(ref: FailsafeRef): string {
    return `${ref.device_uid}/${ref.kind}/${ref.name}`
}
