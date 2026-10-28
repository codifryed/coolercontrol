// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { ChannelInfo } from '@/models/ChannelInfo'
import { plainToInstance, Transform, Type } from 'class-transformer'
import { TempInfo } from '@/models/TempInfo.ts'
import i18n from '@/i18n'

export enum DriverType {
    KERNEL = 'Kernel',
    LIQUIDCTL = 'Liquidctl',
    NVML = 'NVML',
    NVIDIA_CLI = 'NvidiaCLI',
    COOLERCONTROL = 'CoolerControl', // For things like CustomSensors
    EXTERNAL = 'External', // For external device services
}

export class DriverInfo {
    readonly drv_type: DriverType = DriverType.COOLERCONTROL
    readonly name?: string
    readonly version?: string
    readonly locations: string[] = []
}

export class DeviceInfo {
    // We need a special transformer for this collection mapping to work
    @Transform(
        ({ value }) => {
            const result: Map<string, ChannelInfo> = new Map()
            const valueMap = new Map(Object.entries(value))
            for (const [k, v] of valueMap) {
                result.set(k, plainToInstance(ChannelInfo, v))
            }
            return result
        },
        { toClassOnly: true },
    )
    channels: Map<string, ChannelInfo> = new Map<string, ChannelInfo>()

    @Transform(
        ({ value }) => {
            const result: Map<string, TempInfo> = new Map()
            const valueMap = new Map(Object.entries(value))
            for (const [k, v] of valueMap) {
                result.set(k, plainToInstance(TempInfo, v))
            }
            return result
        },
        { toClassOnly: true },
    )
    temps: Map<string, TempInfo> = new Map<string, TempInfo>()

    readonly lighting_speeds: string[] = []
    readonly temp_min: number = 20
    readonly temp_max: number = 100
    readonly profile_max_length: number = 17 // reasonable default, one control point every 5 degrees for 20-100 range
    readonly profile_min_length: number = 2
    readonly model?: string
    readonly thinkpad_fan_control?: boolean
    readonly amd_gpu_overdrive?: boolean
    // Present on a CPU device that can show a frequency per thread. True when they are on.
    readonly per_thread_freq?: boolean | null

    @Type(() => DriverInfo)
    readonly driver_info: DriverInfo

    constructor(
        channels: Map<string, ChannelInfo> = new Map<string, ChannelInfo>(),
        lighting_speeds: string[] = [],
        temp_min: number = 20,
        temp_max: number = 100,
        profile_max_length: number = 17, // reasonable default, one control point every 5 degrees for 20-100 range
        profile_min_length: number = 2,
        model?: string,
        thinkpad_fan_control?: boolean,
        amd_gpu_overdrive?: boolean,
        driver_info: DriverInfo = new DriverInfo(),
    ) {
        this.channels = channels
        this.lighting_speeds = lighting_speeds
        this.temp_min = temp_min
        this.temp_max = temp_max
        this.profile_max_length = profile_max_length
        this.profile_min_length = profile_min_length
        this.model = model
        this.thinkpad_fan_control = thinkpad_fan_control
        this.amd_gpu_overdrive = amd_gpu_overdrive
        this.driver_info = driver_info
    }
}

/**
 * 获取DriverType的本地化显示名称
 * @param type DriverType枚举值
 * @returns 本地化的显示名称
 */
export function getDriverTypeDisplayName(type: DriverType): string {
    const { t } = i18n.global
    switch (type) {
        case DriverType.KERNEL:
            return t('models.driverType.kernel')
        case DriverType.LIQUIDCTL:
            return t('models.driverType.liquidctl')
        case DriverType.NVML:
            return t('models.driverType.nvml')
        case DriverType.NVIDIA_CLI:
            return t('models.driverType.nvidiaCli')
        case DriverType.COOLERCONTROL:
            return t('models.driverType.coolercontrol')
        case DriverType.EXTERNAL:
            return t('models.driverType.external')
        default:
            return String(type)
    }
}
