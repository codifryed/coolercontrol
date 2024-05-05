/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
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

/**
 * Our internal representation of the DeviceSettingsDTO data
 */
export class DaemonDeviceSettings {
    settings: Map<string, DeviceSettingReadDTO> = new Map()
}

export type AllDaemonDeviceSettings = Map<UID, DaemonDeviceSettings>

export class DeviceSettingsReadDTO {
    @Type(() => DeviceSettingReadDTO)
    settings: Array<DeviceSettingReadDTO> = []
}

/**
 * Setting is a passed struct used to apply various settings to a specific device.
 * Usually only one specific lighting or speed setting is applied at a time.
 * This model is for Read only purposes and all possibilities come. To Write settings, use {DeviceSettingWriteDTO}.
 */
export class DeviceSettingReadDTO {
    channel_name: string

    /**
     * The fixed duty speed to set. eg: 20 (%)
     */
    @Transform(({ value }) => (value != null ? Math.round(value) : value))
    speed_fixed?: number

    /**
     * The Profile UID applied to this device channel.
     */
    profile_uid?: UID

    /**
     * Settings for lighting
     */
    @Type(() => LightingSettings)
    lighting?: LightingSettings

    /**
     * Settings for LCD screens
     */
    @Type(() => LcdSettings)
    lcd?: LcdSettings

    /**
     * the current pwm_mode to set for hwmon devices, eg: 1
     */
    pwm_mode?: number

    /**
     * Used to set hwmon & nvidia channels back to their default 'automatic' values.
     */
    reset_to_default?: boolean

    constructor(channelName: string) {
        this.channel_name = channelName
    }
}

/**
 * This DTO is used to write the specific configuration to the daemon.
 */
export class DeviceSettingWriteManualDTO {
    speed_fixed: number

    constructor(speed_fixed: number) {
        this.speed_fixed = speed_fixed
    }
}

/**
 * This DTO is used to write the specific configuration to the daemon.
 */
export class DeviceSettingWriteProfileDTO {
    profile_uid: UID

    constructor(profile_uid: UID) {
        this.profile_uid = profile_uid
    }
}

/**
 * This DTO is used to write the specific configuration to the daemon.
 */
export class DeviceSettingWritePWMModeDTO {
    pwm_mode: number

    constructor(pwm_mode: number) {
        this.pwm_mode = pwm_mode
    }
}

export class TempSource {
    /**
     * The internal name for this Temperature Source. Not the frontend_name or external_name
     */
    temp_name: string

    /**
     * The associated device uid containing current temp values
     */
    device_uid: UID

    constructor(deviceUid: UID, tempName: string) {
        this.device_uid = deviceUid
        this.temp_name = tempName
    }
}

export class LightingSettings {
    /**
     * The lighting mode name
     */
    mode: string

    /**
     * The speed to set
     */
    speed?: string

    /**
     * run backwards or not
     */
    backward?: boolean

    /**
     * a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
     */
    colors: Array<[number, number, number]> = []

    constructor(mode: string) {
        this.mode = mode
    }
}

/**
 * This DTO is used to write the specific configuration to the daemon.
 */
export class DeviceSettingWriteLightingDTO extends LightingSettings {}

export class LcdSettings {
    /**
     * The Lcd mode name
     */
    mode: string

    /**
     * The LCD brightness (0-100%)
     */
    brightness?: number

    /**
     * The LCD Image orientation (0,90,180,270)
     */
    orientation?: number

    /**
     * The LCD Source Image file path location
     */
    image_file_src?: string

    /**
     * The LCD Image tmp file path location, where the preprocessed image is located
     */
    image_file_processed?: string

    /**
     * a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
     */
    colors: Array<[number, number, number]> = []

    /**
     * A temp source for displaying a temperature.
     */
    @Type(() => TempSource)
    temp_source?: TempSource

    constructor(mode: string) {
        this.mode = mode
    }
}

/**
 * This DTO is used to write the specific configuration to the daemon.
 */
export class DeviceSettingWriteLcdDTO extends LcdSettings {}
