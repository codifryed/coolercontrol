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

import type { UID } from './Device'
import { Type } from 'class-transformer'

export class CustomSensor {
    id: String

    cs_type: CustomSensorType
    mix_function: CustomSensorMixFunctionType

    @Type(() => CustomTempSourceData)
    sources: Array<CustomTempSourceData>

    file_path?: string

    constructor(
        id: String,
        cs_type: CustomSensorType = CustomSensorType.Mix,
        mix_function: CustomSensorMixFunctionType = CustomSensorMixFunctionType.Max,
        sources: Array<CustomTempSourceData> = [],
        file_path: string | undefined = undefined,
    ) {
        this.id = id
        this.cs_type = cs_type
        this.mix_function = mix_function
        this.sources = sources
        this.file_path = file_path
    }
}

export enum CustomSensorType {
    Mix = 'Mix',
    File = 'File',
}

export enum CustomSensorMixFunctionType {
    Min = 'Min',
    Max = 'Max',
    Delta = 'Delta',
    Avg = 'Avg',
    WeightedAvg = 'WeightedAvg',
}

export type Weight = number

export class CustomTempSourceData {
    @Type(() => CustomSensorTempSource)
    temp_source: CustomSensorTempSource
    weight: Weight

    constructor(temp_source: CustomSensorTempSource, weight: Weight = 1) {
        this.temp_source = temp_source
        this.weight = weight
    }
}

export class CustomSensorTempSource {
    constructor(
        /**
         * The associated device uid containing current temp values for this source
         */
        readonly device_uid: UID,
        /**
         * The internal name for this Temperature Source. Not the frontend_name or external_name
         */
        readonly temp_name: string,
    ) {}
}
