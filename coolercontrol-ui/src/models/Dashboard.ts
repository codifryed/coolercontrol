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

import { UID } from '@/models/Device.ts'
import { v4 as uuidV4 } from 'uuid'
import { Type } from 'class-transformer'

export class Dashboard {
    uid: UID = uuidV4()
    name: string

    // Type of chart this dashboard has
    chartType: ChartType = ChartType.TIME_CHART

    // time range for chart, if time-chart
    timeRangeSeconds: number = 60

    // Left-side duty/temp scale we call the Degree Scale.
    // Right-side rpm/mhz scale we call the Frequency Scale.

    // auto-scale or static scale...
    autoScaleDegree: boolean = false
    autoScaleFrequency: boolean = true
    autoScaleWatts: boolean = true

    // These are the scale min & maxes used when using a static Degree Axis Scale
    degreeMax: number = 100
    degreeMin: number = 0

    // These are the scale min & maxes used when using a static Frequency Axis Scale
    // These values stay in Mhz/rpms units and are not affected by frequency precision
    frequencyMax: number = 10_000
    frequencyMin: number = 0

    wattsMax: number = 800
    wattsMin: number = 0

    // Selected data types to filter by
    dataTypes: Array<DataType> = []

    // Selected Raw deviceUID and channel names to filter by (not user-level names)
    @Type(() => DashboardDeviceChannel)
    deviceChannelNames: Array<DashboardDeviceChannel> = []

    constructor(dashboardName: string) {
        this.name = dashboardName
    }

    static default(): Dashboard {
        return new Dashboard('System')
    }
}

export enum ChartType {
    TIME_CHART = 'Time Chart',
    TABLE = 'Table',
}

export enum DataType {
    TEMP = 'Temp',
    DUTY = 'Duty',
    LOAD = 'Load',
    RPM = 'RPM',
    FREQ = 'Freq',
    WATTS = 'Watts',
}

export class DashboardDeviceChannel {
    deviceUID: UID
    channelName: string

    constructor(deviceUID: UID, channelName: string) {
        this.deviceUID = deviceUID
        this.channelName = channelName
    }
}
