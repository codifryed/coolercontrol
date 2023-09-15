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
    constructor(
            public readonly name: string,
            public readonly temp: number,
            public readonly frontendName: string,
            public readonly externalName: string
    ) {
    }
}

export class ChannelStatus {
    constructor(
            public readonly name: string,
            public readonly rpm?: number,
            public readonly duty?: number,
            public readonly pwmMode?: number
    ) {
    }
}

/**
 * A Model which contains various applicable device statuses
 */
export class Status {

    @Type(() => Date)
    readonly timestamp: Date = new Date();

    @Type(() => TempStatus)
    readonly temps: Array<TempStatus> = [];

    @Type(() => ChannelStatus)
    readonly channels: Array<ChannelStatus> = [];

    constructor(
            timestamp: Date = new Date(),
            temps: Array<TempStatus> = [],
            channels: Array<ChannelStatus> = []
    ) {
        this.channels = channels;
        this.temps = temps;
        this.timestamp = timestamp;
    }
}
