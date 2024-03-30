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

import { Type } from 'class-transformer'
import { Device, DeviceType, type TypeIndex, type UID } from '@/models/Device'
import { Status } from '@/models/Status'

export class DeviceResponseDTO {
    @Type(() => Device)
    public devices: Device[]

    constructor(devices: Device[] = []) {
        this.devices = devices
    }
}

export class StatusResponseDTO {
    @Type(() => DeviceStatusDTO)
    devices: DeviceStatusDTO[]

    constructor(devices: DeviceStatusDTO[]) {
        this.devices = devices
    }
}

export class DeviceStatusDTO {
    uid: UID
    type: DeviceType
    type_index: TypeIndex

    @Type(() => Status)
    status_history: Status[]

    constructor(type: DeviceType, type_index: TypeIndex, uid: UID, status_history: Status[]) {
        this.type = type
        this.type_index = type_index
        this.uid = uid
        this.status_history = status_history
    }
}
