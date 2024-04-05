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

import { DeviceInfo } from '@/models/DeviceInfo'
import { LcInfo } from '@/models/LcInfo'
import { Status } from '@/models/Status'
import { Type } from 'class-transformer'

export enum DeviceType {
    CUSTOM_SENSORS = 'CustomSensors',
    CPU = 'CPU',
    GPU = 'GPU',
    LIQUIDCTL = 'Liquidctl',
    HWMON = 'Hwmon',
}

export type UID = string
export type TypeIndex = number
export type Color = string

export class Device {
    public readonly uid: UID
    public readonly name: string
    public readonly type: DeviceType
    public readonly type_index: TypeIndex

    @Type(() => LcInfo)
    public readonly lc_info?: LcInfo

    @Type(() => DeviceInfo)
    public readonly info?: DeviceInfo

    @Type(() => Status)
    public status_history: Array<Status> = []

    constructor(
        uid: UID,
        name: string,
        type: DeviceType,
        type_index: TypeIndex,
        lc_info?: LcInfo,
        info?: DeviceInfo,
        status_history: Status[] = [],
    ) {
        this.status_history = status_history
        this.info = info
        this.lc_info = lc_info
        this.type_index = type_index
        this.type = type
        this.name = name
        this.uid = uid
    }

    get nameShort(): string {
        return this.name.split(' (')[0]
    }

    get status(): Status {
        // @ts-ignore
        return this.status_history.at(-1)
    }

    set status(status: Status) {
        this.status_history.push(status)
    }
}
