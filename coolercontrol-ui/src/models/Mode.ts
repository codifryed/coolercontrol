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

import type { UID } from '@/models/Device.ts'
import { DeviceSettingReadDTO } from '@/models/DaemonSettings.ts'
import { Type } from 'class-transformer'

export class Mode {
    uid: UID

    name: string

    @Type(() => DeviceSettingReadDTO)
    device_settings: Array<[UID, Array<DeviceSettingReadDTO>]> = []

    constructor(
        uid: UID,
        name: string,
        device_settings: Array<[UID, Array<DeviceSettingReadDTO>]>,
    ) {
        this.uid = uid
        this.name = name
        this.device_settings = device_settings
    }
}

export class ModesDTO {
    @Type(() => Mode)
    modes: Array<Mode> = []
}

export class ModeOrderDTO {
    mode_uids: Array<UID> = []
}

export class UpdateModeDTO {
    uid: UID
    name: string

    constructor(uid: UID, name: string) {
        this.uid = uid
        this.name = name
    }
}

export class CreateModeDTO {
    name: string

    constructor(name: string) {
        this.name = name
    }
}

export class ActiveModeDTO {
    mode_uid?: UID = undefined
}
