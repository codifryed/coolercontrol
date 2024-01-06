/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import { ChannelInfo } from '@/models/ChannelInfo'
import { plainToInstance, Transform } from 'class-transformer'

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

    readonly lighting_speeds: string[] = []
    readonly temp_min: number = 20
    readonly temp_max: number = 100
    readonly temp_ext_available: boolean = false
    readonly profile_max_length: number = 17 // reasonable default, one control point every 5 degrees for 20-100 range
    readonly profile_min_length: number = 2
    readonly model?: string
    readonly thinkpad_fan_control?: boolean

    constructor(
        channels: Map<string, ChannelInfo> = new Map<string, ChannelInfo>(),
        lighting_speeds: string[] = [],
        temp_min: number = 20,
        temp_max: number = 100,
        temp_ext_available: boolean = false,
        profile_max_length: number = 17, // reasonable default, one control point every 5 degrees for 20-100 range
        profile_min_length: number = 2,
        model?: string,
        thinkpad_fan_control?: boolean,
    ) {
        this.channels = channels
        this.lighting_speeds = lighting_speeds
        this.temp_min = temp_min
        this.temp_max = temp_max
        this.temp_ext_available = temp_ext_available
        this.profile_max_length = profile_max_length
        this.profile_min_length = profile_min_length
        this.model = model
        this.thinkpad_fan_control = thinkpad_fan_control
    }
}
