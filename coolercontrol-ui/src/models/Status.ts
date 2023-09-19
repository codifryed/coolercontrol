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


import {Type} from "class-transformer";

export class TempStatus {
    readonly name: string
    readonly temp: number
    readonly frontend_name: string
    readonly external_name: string

    constructor(
        name: string,
        temp: number,
        frontend_name: string,
        external_name: string
    ) {
        this.name = name
        this.temp = temp
        this.frontend_name = frontend_name
        this.external_name = external_name
    }
}

export class ChannelStatus {
    readonly name: string
    readonly rpm?: number
    readonly duty?: number
    readonly pwm_mode?: number

    constructor(
        name: string,
        rpm?: number,
        duty?: number,
        pwm_mode?: number
    ) {
        this.name = name
        this.rpm = rpm
        this.duty = duty
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
        channels: Array<ChannelStatus> = []
    ) {
        this.channels = channels
        this.temps = temps
        this.timestamp = timestamp
    }
}
