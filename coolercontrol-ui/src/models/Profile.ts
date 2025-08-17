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

import { Transform, Type } from 'class-transformer'
import type { UID } from '@/models/Device'
// @ts-ignore
import { v4 as uuidV4 } from 'uuid'
import i18n from '@/i18n'

/**
 * This is currently an internal model that will be also used externally by daemon at some point. The existing external
 * model will be transformed into this one until then.
 */
export class Profile {
    /**
     * The Unique identifier for this Profile
     */
    uid: UID = uuidV4()

    /**
     * The type of this Profile
     */
    p_type: ProfileType = ProfileType.Default

    /**
     * The name of this Profile
     */
    name: string

    /**
     * The fixed duty speed to set. eg: 20 (%)
     */
    speed_fixed?: number

    /**
     * The profile temp/duty speeds to set. eg: `[(20, 50), (25, 80)]`
     */
    @Transform(({ value }) => {
        const profile: Array<[number, number]> | undefined = value
        if (profile != null) {
            for (const point of profile) {
                // temp:
                point[0] = Math.round(point[0] * 10) / 10
                // duty:
                point[1] = Math.round(point[1])
            }
        }
        return profile
    })
    speed_profile: Array<[number, number]> = []

    /**
     * The associated temperature source
     */
    @Type(() => ProfileTempSource)
    temp_source?: ProfileTempSource

    /**
     * The minimum temp for this profile
     */
    temp_min?: number

    /**
     * The maximum temp for this profile
     */
    temp_max?: number

    /**
     * The function UID to apply to this profile
     */
    function_uid: UID = '0'

    /**
     * The profiles that make up the Mix ProfileType
     */
    member_profile_uids: Array<UID> = []

    /**
     * The function to mix the members with if this is a Mix ProfileType
     */
    mix_function_type?: ProfileMixFunctionType

    /**
     * The graph offset to apply to the associated member profile
     * This can also be used as a static offset when there is only one duty/offset pair.
     */
    @Transform(({ value }) => {
        const profile: Array<[number, number]> | undefined = value
        if (profile != null) {
            for (const point of profile) {
                // duty:
                point[0] = Math.round(point[0])
                // offset:
                point[1] = Math.round(point[1])
            }
        }
        return profile
    })
    offset_profile: Array<[number, number]> = []

    constructor(
        name: string,
        type: ProfileType,
        speed_fixed?: number,
        temp_source?: ProfileTempSource,
        speed_profile: Array<[number, number]> = [],
        member_profile_uids: Array<UID> = [],
        mix_function_type: ProfileMixFunctionType | undefined = undefined,
        offset_profile: Array<[number, number]> = [],
    ) {
        this.name = name
        this.p_type = type
        this.speed_fixed = speed_fixed
        this.speed_profile = speed_profile
        this.temp_source = temp_source
        this.member_profile_uids = member_profile_uids
        this.mix_function_type = mix_function_type
        this.offset_profile = offset_profile
    }

    static createDefault(): Profile {
        const profile = new Profile('Default Profile', ProfileType.Default)
        profile.uid = '0' // this indicates a special once-only non-deleteable default profile that we always need to have available
        return profile
    }
}

export enum ProfileType {
    Default = 'Default',
    Fixed = 'Fixed',
    Graph = 'Graph',
    Mix = 'Mix',
    Overlay = 'Overlay',
}

/**
 * Get localized display name for ProfileType
 */
export function getProfileTypeDisplayName(type: ProfileType): string {
    const t = i18n.global.t
    switch (type) {
        case ProfileType.Default:
            return t('models.profile.profileType.default')
        case ProfileType.Fixed:
            return t('models.profile.profileType.fixed')
        case ProfileType.Graph:
            return t('models.profile.profileType.graph')
        case ProfileType.Mix:
            return t('models.profile.profileType.mix')
        case ProfileType.Overlay:
            return t('models.profile.profileType.overlay')
        default:
            return type
    }
}

export class ProfileTempSource {
    constructor(
        /**
         * The internal name for this Temperature Source. Not the frontend_name or external_name
         */
        readonly temp_name: string,
        /**
         * The associated device uid containing current temp values
         */
        readonly device_uid: UID,
    ) {}
}

export class ProfilesDTO {
    @Type(() => Profile)
    profiles: Array<Profile> = []
}

export enum FunctionType {
    Identity = 'Identity',
    Standard = 'Standard',
    ExponentialMovingAvg = 'ExponentialMovingAvg',
}

/**
 * Get localized display name for FunctionType
 */
export function getFunctionTypeDisplayName(type: FunctionType): string {
    const t = i18n.global.t
    switch (type) {
        case FunctionType.Identity:
            return t('models.profile.functionType.identity')
        case FunctionType.Standard:
            return t('models.profile.functionType.standard')
        case FunctionType.ExponentialMovingAvg:
            return t('models.profile.functionType.exponentialMovingAvg')
        default:
            return type
    }
}

export class Function {
    /**
     * The Unique identifier for this function
     */
    uid: UID = uuidV4()

    /**
     * The user given name for this function
     */
    name: string

    /**
     * The type of this function
     */
    f_type: FunctionType = FunctionType.Identity

    /**
     * The minimum duty change to apply
     */
    duty_minimum: number

    /**
     * The maximum duty change to apply
     */
    duty_maximum: number

    /**
     * The response delay in seconds
     */
    response_delay?: number

    /**
     * The temperature deviance threshold in degrees
     */
    deviance?: number

    /**
     * Whether to apply the settings only on the way down
     */
    only_downward?: boolean

    /**
     * The sample window this function should use, particularly applicable to moving averages
     */
    sample_window?: number

    constructor(
        name: string,
        f_type: FunctionType = FunctionType.Identity,
        duty_minimum: number = 2,
        duty_maximum: number = 100,
        response_delay: number | undefined = undefined,
        deviance: number | undefined = undefined,
        only_downward: boolean | undefined = undefined,
        sample_window: number | undefined = undefined,
    ) {
        this.name = name
        this.f_type = f_type
        this.duty_minimum = duty_minimum
        this.duty_maximum = duty_maximum
        this.response_delay = response_delay
        this.deviance = deviance
        this.only_downward = only_downward
        this.sample_window = sample_window
    }

    static createDefault(): Function {
        return new Function('Identity')
    }
}

export class FunctionsDTO {
    @Type(() => Function)
    functions: Array<Function> = []
}

export enum ProfileMixFunctionType {
    Min = 'Min',
    Max = 'Max',
    Avg = 'Avg',
    Diff = 'Diff',
}

/**
 * Get localized display name for ProfileMixFunctionType
 */
export function getProfileMixFunctionTypeDisplayName(type: ProfileMixFunctionType): string {
    const t = i18n.global.t
    switch (type) {
        case ProfileMixFunctionType.Min:
            return t('models.profile.mixFunctionType.min')
        case ProfileMixFunctionType.Max:
            return t('models.profile.mixFunctionType.max')
        case ProfileMixFunctionType.Avg:
            return t('models.profile.mixFunctionType.avg')
        case ProfileMixFunctionType.Diff:
            return t('models.profile.mixFunctionType.diff')
        default:
            return type
    }
}
