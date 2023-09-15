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


import {DefaultDictionary} from "typescript-collections";
import {DeviceInfo} from "@/models/DeviceInfo";
import {LcInfo} from "@/models/LcInfo";
import {Status} from "@/models/Status";
import {Type} from "class-transformer";

export enum DeviceType {
    CPU = 'CPU',
    GPU = 'GPU',
    LIQUIDCTL = 'Liquidctl',
    HWMON = 'Hwmon',
    COMPOSITE = 'Composite',
}

export type UID = string
export type TypeIndex = number
export type HexColor = string

export class Device {

    public readonly uid: UID;
    public readonly name: string;
    public readonly type: DeviceType;
    public readonly typeIndex: TypeIndex;

    @Type(() => LcInfo)
    public readonly lcInfo?: LcInfo;

    @Type(() => DeviceInfo)
    public readonly info?: DeviceInfo;

    /**
     * A Map of ChannelName to HexColor values
     */
    public colors: DefaultDictionary<string, HexColor> = new DefaultDictionary((): HexColor => "#568af2");

    @Type(() => Status)
    public statusHistory: Array<Status> = [];

    constructor(uid: UID,
                name: string,
                type: DeviceType,
                typeIndex: TypeIndex,
                lcInfo?: LcInfo,
                info?: DeviceInfo,
                colors: DefaultDictionary<string, HexColor> = new DefaultDictionary((): HexColor => "#568af2"),
                statusHistory: Status[] = [],
    ) {
        this.statusHistory = statusHistory;
        this.colors = colors;
        this.info = info;
        this.lcInfo = lcInfo;
        this.typeIndex = typeIndex;
        this.type = type;
        this.name = name;
        this.uid = uid;
    }

    get nameShort(): string {
        return this.name.split(' (')[0]
    }

    get status(): Status {
        // todo: I think this should work, it's just not being picked up as set to 'ESNext'
        // @ts-ignore
        return this.statusHistory.at(-1)
    }

    set status(status: Status) {
        this.statusHistory.push(status)
    }

    colorForChannel(channelName: string): HexColor {
        return this.colors.getValue(channelName)
    }
}