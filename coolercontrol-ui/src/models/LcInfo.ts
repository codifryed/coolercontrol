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

import { LcDriverType } from '@/models/LcDriverType'

export class LcInfo {
    readonly driver_type: LcDriverType
    readonly firmware_version: string
    readonly unknown_asetek: boolean

    constructor(driver_type: LcDriverType, firmware_version: string, unknown_asetek: boolean) {
        this.driver_type = driver_type
        this.firmware_version = firmware_version
        this.unknown_asetek = unknown_asetek
    }
}
