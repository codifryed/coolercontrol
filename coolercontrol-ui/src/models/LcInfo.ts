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

import {LcDriverType} from "@/models/LcDriverType";

export class LcInfo {
    readonly driverType: LcDriverType
    readonly firmwareVersion: string
    readonly unknownAsetek: boolean

    constructor(
            driverType: LcDriverType,
            firmwareVersion: string,
            unknownAsetek: boolean
    ) {
        this.driverType = driverType
        this.firmwareVersion = firmwareVersion
        this.unknownAsetek = unknownAsetek
    }
}