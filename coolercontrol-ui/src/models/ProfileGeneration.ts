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

import { Type } from 'class-transformer'
import type { UID } from '@/models/Device'
import { ProfileTempSource, Profile, Function } from '@/models/Profile'
import { CustomSensor } from '@/models/CustomSensor'

/**
 * Contract for the auto-create-profiles wizard. Mirrors the daemon's
 * `api::profile_generation` types. The string values MUST match the daemon's PascalCase
 * serialization exactly.
 */

/** The cooling role a fan plays. Assigned explicitly by the user. */
export enum FanKind {
    CpuCooler = 'CpuCooler',
    GpuFan = 'GpuFan',
    AioRadiator = 'AioRadiator',
    AioPump = 'AioPump',
    CaseIntake = 'CaseIntake',
    CaseExhaust = 'CaseExhaust',
    LaptopFan = 'LaptopFan',
}

/** The noise/performance tradeoff applied to a generated profile. */
export enum Preset {
    Silent = 'Silent',
    Balanced = 'Balanced',
    Performance = 'Performance',
}

/** A case-fan mounting position. Label only: it affects generated names, not the curve. */
export enum FanPosition {
    Top = 'Top',
    Front = 'Front',
    Back = 'Back',
    Bottom = 'Bottom',
}

/** Which temperature a laptop fan follows. Honored only when the kind is `LaptopFan`. */
export enum LaptopTempStrategy {
    EmaCpu = 'EmaCpu',
    ThinkpadSensor = 'ThinkpadSensor',
    MixCpuGpu = 'MixCpuGpu',
}

/** One fan the user has assigned a cooling role to. Skipped fans are omitted from the request. */
export class FanAssignment {
    device_uid: UID
    channel_name: string
    kind: FanKind
    position?: FanPosition
    laptop_temp_strategy?: LaptopTempStrategy

    constructor(
        device_uid: UID,
        channel_name: string,
        kind: FanKind,
        position?: FanPosition,
        laptop_temp_strategy?: LaptopTempStrategy,
    ) {
        this.device_uid = device_uid
        this.channel_name = channel_name
        this.kind = kind
        this.position = position
        this.laptop_temp_strategy = laptop_temp_strategy
    }
}

/** The canonical system temps the user has identified. Each is optional. */
export class KeyTemps {
    cpu?: ProfileTempSource
    gpu?: ProfileTempSource
    liquid?: ProfileTempSource
    ambient?: ProfileTempSource
}

/** A per-kind preset that overrides the global preset for one kind. */
export class PresetOverride {
    kind: FanKind
    preset: Preset

    constructor(kind: FanKind, preset: Preset) {
        this.kind = kind
        this.preset = preset
    }
}

/** The full input to one profile-generation run. */
export class GenerateProfilesRequest {
    @Type(() => FanAssignment)
    assignments: Array<FanAssignment> = []

    @Type(() => KeyTemps)
    key_temps: KeyTemps = new KeyTemps()

    global_preset: Preset = Preset.Balanced

    @Type(() => PresetOverride)
    preset_overrides: Array<PresetOverride> = []
}

/** A fan-to-profile assignment the run proposes. */
export class ChannelAssignment {
    device_uid: UID = ''
    channel_name: string = ''
    profile_uid: UID = ''

    /** Set when the channel already has a non-default profile that Create & Apply would replace. */
    replaces_profile_name?: string
}

/** The proposed result of a generation run. Nothing here is persisted until the user confirms. */
export class GenerateProfilesResponse {
    @Type(() => CustomSensor)
    custom_sensors: Array<CustomSensor> = []

    @Type(() => Function)
    functions: Array<Function> = []

    @Type(() => Profile)
    profiles: Array<Profile> = []

    @Type(() => ChannelAssignment)
    assignments: Array<ChannelAssignment> = []
}
