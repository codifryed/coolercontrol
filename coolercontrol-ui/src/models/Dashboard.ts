/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
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
import i18n from '@/i18n'

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
    dataTypes: Array<DataType> = [DataType.TEMP, DataType.DUTY, DataType.LOAD]

    // Selected Raw deviceUID and channel names to filter by (not user-level names)
    @Type(() => DashboardDeviceChannel)
    deviceChannelNames: Array<DashboardDeviceChannel> = []

    constructor(dashboardName: string) {
        this.name = dashboardName
    }

    // Dashboards build by default when there are none (first-run)
    static defaults(): Array<Dashboard> {
        return [new Dashboard('System')]
    }
}

export enum ChartType {
    TIME_CHART = 'Time Chart',
    TABLE = 'Table',
    CONTROLS = 'Controls',
}

// Get localized chart type names
export function getLocalizedChartType(type: ChartType): string {
    const { t } = i18n.global
    switch (type) {
        case ChartType.TIME_CHART:
            return t('models.chartType.timeChart')
        case ChartType.TABLE:
            return t('models.chartType.table')
        case ChartType.CONTROLS:
            return t('models.chartType.controls')
        default:
            return type
    }
}

export enum DataType {
    TEMP = 'Temp',
    DUTY = 'Duty',
    LOAD = 'Load',
    RPM = 'RPM',
    FREQ = 'Freq',
    WATTS = 'Watts',
}

// Get localized data type names
export function getLocalizedDataType(type: DataType): string {
    const { t } = i18n.global
    switch (type) {
        case DataType.TEMP:
            return t('models.dataType.temp')
        case DataType.DUTY:
            return t('models.dataType.duty')
        case DataType.LOAD:
            return t('models.dataType.load')
        case DataType.RPM:
            return t('models.dataType.rpm')
        case DataType.FREQ:
            return t('models.dataType.freq')
        case DataType.WATTS:
            return t('models.dataType.watts')
        default:
            return type
    }
}

export class DashboardDeviceChannel {
    deviceUID: UID
    channelName: string

    constructor(deviceUID: UID, channelName: string) {
        this.deviceUID = deviceUID
        this.channelName = channelName
    }
}
