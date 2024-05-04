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

export class TempStatus {
    readonly name: string
    readonly temp: number

    constructor(name: string, temp: number) {
        this.name = name
        this.temp = temp
    }
}

export class ChannelStatus {
    readonly name: string
    readonly rpm?: number
    // We want to deal with duty values as integers in the UI. Reasons being ease of display and that we need to also set
    // duty values as integers and not floats. Duties come as floats due to the fact that there is a calculation in the
    // backend from the raw bit values to product 0-100 duty percent values, and floats are very useful for backend
    // calculations without having to convert those number types all the time.
    // Due to the above, the best and easiest approach currently is to be to round the duty values in the UI.
    @Transform(({ value }) => (value != null ? Math.round(value) : value))
    readonly duty?: number
    readonly freq?: number
    readonly pwm_mode?: number

    constructor(name: string, rpm?: number, duty?: number, freq?: number, pwm_mode?: number) {
        this.name = name
        this.rpm = rpm
        this.duty = duty
        this.freq = freq
        this.pwm_mode = pwm_mode
    }
}

/**
 * A Model which contains various applicable device statuses
 */
export class Status {
    readonly timestamp: string

    @Type(() => TempStatus)
    readonly temps: Array<TempStatus> = []

    @Type(() => ChannelStatus)
    readonly channels: Array<ChannelStatus> = []

    constructor(
        timestamp: string,
        temps: Array<TempStatus> = [],
        channels: Array<ChannelStatus> = [],
    ) {
        this.channels = channels
        this.temps = temps
        this.timestamp = timestamp
    }
}
