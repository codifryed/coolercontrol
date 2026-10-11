// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { UID } from './Device'
import { Type } from 'class-transformer'
import i18n from '@/i18n'

export enum CustomSensorType {
    Mix = 'Mix',
    File = 'File',
    Offset = 'Offset',
    TimeAverage = 'TimeAverage',
    ExponentialMovingAvg = 'ExponentialMovingAvg',
}

export enum CustomSensorMixFunctionType {
    Min = 'Min',
    Max = 'Max',
    Delta = 'Delta',
    Avg = 'Avg',
    WeightedAvg = 'WeightedAvg',
    Sum = 'Sum',
}

// The channel metric a sensor reads and reports. All of its sources share it.
export enum CustomSensorMetric {
    Temp = 'Temp',
    Duty = 'Duty',
    RPM = 'RPM',
    Freq = 'Freq',
    Watts = 'Watts',
}

export function getCustomSensorMetricDisplayName(metric: CustomSensorMetric): string {
    const { t } = i18n.global
    switch (metric) {
        case CustomSensorMetric.Temp:
            return t('models.dataType.temp')
        case CustomSensorMetric.Duty:
            return t('models.dataType.duty')
        case CustomSensorMetric.RPM:
            return t('models.dataType.rpm')
        case CustomSensorMetric.Freq:
            return t('models.dataType.freq')
        case CustomSensorMetric.Watts:
            return t('models.dataType.watts')
        default:
            return String(metric)
    }
}

/**
 * 获取CustomSensorType的本地化显示名称
 * @param type CustomSensorType枚举值
 * @returns 本地化的显示名称
 */
export function getCustomSensorTypeDisplayName(type: CustomSensorType): string {
    const { t } = i18n.global
    switch (type) {
        case CustomSensorType.Mix:
            return t('models.customSensor.sensorType.mix')
        case CustomSensorType.File:
            return t('models.customSensor.sensorType.file')
        case CustomSensorType.Offset:
            return t('models.customSensor.sensorType.offset')
        case CustomSensorType.TimeAverage:
            return t('models.customSensor.sensorType.timeAverage')
        case CustomSensorType.ExponentialMovingAvg:
            return t('models.customSensor.sensorType.exponentialMovingAvg')
        default:
            return String(type)
    }
}

/**
 * 获取CustomSensorMixFunctionType的本地化显示名称
 * @param type CustomSensorMixFunctionType枚举值
 * @returns 本地化的显示名称
 */
export function getCustomSensorMixFunctionTypeDisplayName(
    type: CustomSensorMixFunctionType,
): string {
    const { t } = i18n.global
    switch (type) {
        case CustomSensorMixFunctionType.Min:
            return t('models.customSensor.mixFunctionType.min')
        case CustomSensorMixFunctionType.Max:
            return t('models.customSensor.mixFunctionType.max')
        case CustomSensorMixFunctionType.Delta:
            return t('models.customSensor.mixFunctionType.delta')
        case CustomSensorMixFunctionType.Avg:
            return t('models.customSensor.mixFunctionType.avg')
        case CustomSensorMixFunctionType.WeightedAvg:
            return t('models.customSensor.mixFunctionType.weightedAvg')
        case CustomSensorMixFunctionType.Sum:
            return t('models.customSensor.mixFunctionType.sum')
        default:
            return String(type)
    }
}

export type Weight = number

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

export class CustomSensorChannelSource {
    constructor(
        readonly device_uid: UID,
        /**
         * The internal name for this channel. Not the label.
         */
        readonly channel_name: string,
    ) {}
}

// One source of a Custom Sensor. A temperature sensor's sources are temps, under
// `temp_source`. A sensor of any other metric reads channels, under `channel_source`.
export class CustomSensorSourceData {
    @Type(() => CustomSensorTempSource)
    temp_source?: CustomSensorTempSource

    @Type(() => CustomSensorChannelSource)
    channel_source?: CustomSensorChannelSource

    weight: Weight

    constructor(
        weight: Weight = 1,
        temp_source?: CustomSensorTempSource,
        channel_source?: CustomSensorChannelSource,
    ) {
        this.weight = weight
        this.temp_source = temp_source
        this.channel_source = channel_source
    }

    static of(
        metric: CustomSensorMetric,
        deviceUID: UID,
        name: string,
        weight: Weight = 1,
    ): CustomSensorSourceData {
        return metric === CustomSensorMetric.Temp
            ? new CustomSensorSourceData(weight, new CustomSensorTempSource(deviceUID, name))
            : new CustomSensorSourceData(
                  weight,
                  undefined,
                  new CustomSensorChannelSource(deviceUID, name),
              )
    }

    get deviceUID(): UID {
        return this.temp_source?.device_uid ?? this.channel_source?.device_uid ?? ''
    }

    // The temp or channel name on the source device.
    get name(): string {
        return this.temp_source?.temp_name ?? this.channel_source?.channel_name ?? ''
    }
}

export class CustomSensor {
    id: string

    // Fixed once the sensor exists. Absent from older payloads, where it is a temperature.
    metric: CustomSensorMetric = CustomSensorMetric.Temp

    cs_type: CustomSensorType
    mix_function: CustomSensorMixFunctionType
    // Scale & Offset: source * scale + offset.
    scale?: number
    offset?: number
    time_window_seconds?: number

    @Type(() => CustomSensorSourceData)
    sources: Array<CustomSensorSourceData>

    children: Array<string> = []
    parents: Array<string> = []

    file_path?: string

    constructor(
        id: string,
        cs_type: CustomSensorType = CustomSensorType.Mix,
        mix_function: CustomSensorMixFunctionType = CustomSensorMixFunctionType.Max,
        sources: Array<CustomSensorSourceData> = [],
        file_path: string | undefined = undefined,
    ) {
        this.id = id
        this.cs_type = cs_type
        this.mix_function = mix_function
        this.sources = sources
        this.file_path = file_path
    }
}
