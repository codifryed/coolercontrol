// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { UID } from '@/models/Device'
import { ChannelExtensionNames } from '@/models/SpeedOptions.ts'
import { plainToInstance, Transform, Type } from 'class-transformer'

/**
 * General settings specific to CoolerControl
 */
export class CoolerControlSettingsDTO {
    apply_on_boot: boolean = true
    no_init: boolean = false
    startup_delay: number = 2
    thinkpad_full_speed: boolean = false
    hide_duplicate_devices: boolean = true
    liquidctl_integration: boolean = true
    compress: boolean = false
    poll_rate: number = 1.0
    drivetemp_suspend: boolean = false
    sensors_auto_detect: boolean = true
    device_listener_enabled: boolean = false
    sensors_conf_enabled: boolean = true
}

/**
 * General settings specific to CoolerControl that affect specific devices
 */
export class CoolerControlDeviceSettingsDTO {
    uid: UID
    name: string
    disable: boolean = false

    // Specialized settings (extensions) that apply to a specific device.
    extensions: DeviceExtensions = new DeviceExtensions()

    // We need a special transformer for this collection mapping to work
    @Transform(
        ({ value }) => {
            const result: Map<string, CCChannelSettings> = new Map()
            const valueMap = new Map(Object.entries(value))
            for (const [k, v] of valueMap) {
                result.set(k, plainToInstance(CCChannelSettings, v))
            }
            return result
        },
        { toClassOnly: true },
    )
    channel_settings: Map<string, CCChannelSettings> = new Map<string, CCChannelSettings>()

    constructor(uid: UID, name: string) {
        this.uid = uid
        this.name = name
    }
}

export class CoolerControlAllDeviceSettingsDTO {
    @Type(() => CoolerControlDeviceSettingsDTO)
    devices: Array<CoolerControlDeviceSettingsDTO> = []
}

export class CCChannelSettings {
    label?: string
    disabled: boolean = false

    // Specialized settings (extensions) that apply to a specific device channel.
    extension?: ChannelExtensions
}

// since TS doesn't have the same kind of enum power as Rust, we include all options
export class DeviceExtensions {
    // Whether to enable Direct Access for the liquidctl driver,
    // which will cause liquidctl to ignore the HWMon kernel driver
    direct_access: boolean = false

    // The delay in milliseconds to force between applying settings to this device.
    // This is to help with communication issues with some devices that may not handle
    // multiple settings applied in quick succession. (The driver does not always handle this)
    delay_millis: number = 0
}

// since TS doesn't have the same kind of enum power as Rust, we include all options
export class ChannelExtensions {
    // Whether to use the device channel's internal hardware fan curve functionality.
    auto_hw_curve_enabled?: boolean

    // Whether to use the AMDGPU RDNA3/4 features.
    // Whether to use the internal HW Curve feature, instead of setting regular
    // flat curves. Using this reduces functionality.
    hw_fan_curve_enabled?: boolean
}

// The channel must expose a firmware-curve extension AND carry the matching
// toggle. Each extension has its own flag, so the pairing matters: a stale
// flag from the other extension does not count.
export function isFirmwareCurveEnabled(
    extensionName: ChannelExtensionNames | undefined,
    extensions: ChannelExtensions | undefined,
): boolean {
    switch (extensionName) {
        case ChannelExtensionNames.AutoHWCurve:
            return extensions?.auto_hw_curve_enabled === true
        case ChannelExtensionNames.AmdRdnaGpu:
            return extensions?.hw_fan_curve_enabled === true
        default:
            return false
    }
}
