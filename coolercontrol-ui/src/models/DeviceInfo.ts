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

import {ChannelInfo} from "@/models/ChannelInfo";
import {plainToInstance, Transform} from "class-transformer";
import {Dictionary} from "typescript-collections";

export class DeviceInfo {

    // We need a special transformer for even Map to work, and especially for dictionary
    @Transform(({value}) => {
        const result: Dictionary<string, ChannelInfo> = new Dictionary()
        const valueMap = new Map(Object.entries(value))
        valueMap.forEach((v, k) => {
            result.setValue(k, plainToInstance(ChannelInfo, v))
        })
        return result
    }, {toClassOnly: true})
    readonly channels: Dictionary<string, ChannelInfo> = new Dictionary<string, ChannelInfo>

    readonly lightingSpeeds: string[] = []
    readonly tempMin: number = 20
    readonly tempMax: number = 100
    readonly tempExtAvailable: boolean = false
    readonly profileMaxLength: number = 17 // reasonable default, one control point every 5 degrees for 20-100 range
    readonly profileMinLength: number = 2
    readonly model?: string
    readonly thinkpadFanControl?: boolean

    constructor(
            channels: Dictionary<string, ChannelInfo> = new Dictionary<string, ChannelInfo>(),
            lightingSpeeds: string[] = [],
            tempMin: number = 20,
            tempMax: number = 100,
            tempExtAvailable: boolean = false,
            profileMaxLength: number = 17, // reasonable default, one control point every 5 degrees for 20-100 range
            profileMinLength: number = 2,
            model?: string,
            thinkpadFanControl?: boolean
    ) {
        this.channels = channels
        this.lightingSpeeds = lightingSpeeds
        this.tempMin = tempMin
        this.tempMax = tempMax
        this.tempExtAvailable = tempExtAvailable
        this.profileMaxLength = profileMaxLength
        this.profileMinLength = profileMinLength
        this.model = model
        this.thinkpadFanControl = thinkpadFanControl
    }
}
