// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { v4 as uuidV4 } from 'uuid'
import { type Device, DeviceType, type UID } from '@/models/Device.ts'
import { type CustomSensor, CustomSensorMetric, CustomSensorType } from '@/models/CustomSensor.ts'
import type { Status } from '@/models/Status.ts'
import type { ChannelValues } from '@/stores/DeviceStore.ts'

const ID_ATTEMPTS = 10

const shortHash = (): string => uuidV4().slice(0, 8)

// A counter would hand a deleted sensor's id to the next one, which then inherits the
// dashboards, colors and alerts still pointing at that id.
export function newCustomSensorId(
    takenIds: ReadonlySet<string>,
    hash: () => string = shortHash,
): string {
    for (let attempt = 0; attempt < ID_ATTEMPTS; attempt++) {
        const id = `sensor_${hash()}`
        if (!takenIds.has(id)) return id
    }
    throw new Error('Could not generate a unique Custom Sensor ID')
}

// A temp or channel a sensor can read, as the source pickers show it.
export interface AvailableSource {
    deviceUID: UID
    // The internal temp or channel name.
    name: string
    frontendName: string
    lineColor: string
    weight: number
    // The current value, formatted.
    value: string
}

export interface AvailableSourceGroup {
    deviceUID: UID
    deviceName: string
    sources: Array<AvailableSource>
}

// The display names and colors of a device and its sensors.
export interface SourceLabels {
    name: string
    sensorsAndChannels: Map<string, { name: string; color: string }>
}

// The sensor being edited, as far as the source rules need it.
export interface EditedSensor {
    id: string
    parents: Array<string>
}

// Every temp or channel of `status` that reports `metric`, with its value as displayed.
function metricEntries(metric: CustomSensorMetric, status: Status): Array<[string, string]> {
    if (metric === CustomSensorMetric.Temp) {
        return status.temps.map((temp) => [temp.name, temp.temp.toFixed(1)])
    }
    const entries: Array<[string, string]> = []
    for (const channel of status.channels) {
        const value = channelValue(metric, channel)
        if (value != null) entries.push([channel.name, value])
    }
    return entries
}

function channelValue(
    metric: CustomSensorMetric,
    channel: Status['channels'][number],
): string | undefined {
    switch (metric) {
        case CustomSensorMetric.Duty:
            return channel.duty?.toFixed(0)
        case CustomSensorMetric.RPM:
            return channel.rpm?.toFixed(0)
        case CustomSensorMetric.Freq:
            return channel.freq?.toFixed(0)
        case CustomSensorMetric.Watts:
            return channel.watts?.toFixed(1)
        default:
            return undefined
    }
}

// The live value of `metric` among a temp or channel's current values, as displayed.
export function liveValue(
    metric: CustomSensorMetric,
    values: ChannelValues | undefined,
): string | undefined {
    switch (metric) {
        case CustomSensorMetric.Temp:
            return values?.temp
        case CustomSensorMetric.Duty:
            return values?.duty
        case CustomSensorMetric.RPM:
            return values?.rpm
        case CustomSensorMetric.Freq:
            return values?.freq
        case CustomSensorMetric.Watts:
            return values?.watts
        default:
            return undefined
    }
}

// What a sensor of `metric` can take as a source: every temp for a temperature sensor,
// otherwise every channel reporting that metric. Other Custom Sensors qualify only when
// they share the metric and the one-level hierarchy allows it: a sensor that is already a
// child takes none, and neither itself nor a sensor with children can become its child.
export function availableSources(
    devices: Iterable<Device>,
    metric: CustomSensorMetric,
    customSensors: ReadonlyArray<CustomSensor>,
    edited: EditedSensor,
    labelsOf: (deviceUID: UID) => SourceLabels | undefined,
): Array<AvailableSourceGroup> {
    const groups: Array<AvailableSourceGroup> = []
    for (const device of devices) {
        if (device.info == null) continue
        const isCustomSensors = device.type === DeviceType.CUSTOM_SENSORS
        if (isCustomSensors && edited.parents.length > 0) continue
        const labels = labelsOf(device.uid)
        if (labels == null) continue
        const sources: Array<AvailableSource> = []
        for (const [name, value] of metricEntries(metric, device.status)) {
            if (isCustomSensors && !canBeChild(name, metric, customSensors, edited)) continue
            const display = labels.sensorsAndChannels.get(name)
            if (display == null) continue
            sources.push({
                deviceUID: device.uid,
                name,
                frontendName: display.name,
                lineColor: display.color,
                weight: 1,
                value,
            })
        }
        if (sources.length === 0) continue
        groups.push({ deviceUID: device.uid, deviceName: labels.name, sources })
    }
    return groups
}

function canBeChild(
    sensorId: string,
    metric: CustomSensorMetric,
    customSensors: ReadonlyArray<CustomSensor>,
    edited: EditedSensor,
): boolean {
    if (sensorId === edited.id) return false
    const sensor = customSensors.find((cs) => cs.id === sensorId)
    if (sensor == null) return false
    if (sensor.children.length > 0) return false
    return sensor.metric === metric
}

export const TEMP_OFFSET_LIMIT = 100
export const CHANNEL_OFFSET_LIMIT = 1_000_000
export const SCALE_MAGNITUDE_MIN = 0.000001
export const SCALE_MAGNITUDE_MAX = 1_000_000
export const TIME_WINDOW_SECONDS_MIN = 1
export const TIME_WINDOW_SECONDS_MAX = 300

// The largest offset, in either direction, the daemon takes for a metric.
export function offsetLimit(metric: CustomSensorMetric): number {
    return metric === CustomSensorMetric.Temp ? TEMP_OFFSET_LIMIT : CHANNEL_OFFSET_LIMIT
}

// A scale is any finite factor but 0, with a magnitude the daemon takes. Negative inverts.
export function isUsableScale(scale: number | null | undefined): boolean {
    if (scale == null || !Number.isFinite(scale)) return false
    const magnitude = Math.abs(scale)
    return magnitude >= SCALE_MAGNITUDE_MIN && magnitude <= SCALE_MAGNITUDE_MAX
}

// What the editor holds when Save is pressed.
export interface EditorState {
    type: CustomSensorType
    metric: CustomSensorMetric
    mixSourceCount: number
    hasSingleSource: boolean
    filePath: string | null | undefined
    scale: number | null | undefined
    offset: number | null | undefined
    timeWindowSeconds: number | null | undefined
}

// Whether the daemon would take the sensor as entered, so Save can be offered.
export function canSave(state: EditorState): boolean {
    switch (state.type) {
        case CustomSensorType.Mix:
            return state.mixSourceCount > 0
        case CustomSensorType.File:
            return (state.filePath ?? '').trim().length > 0
        case CustomSensorType.Offset:
            return (
                state.hasSingleSource &&
                isUsableScale(state.scale) &&
                state.offset != null &&
                Math.abs(state.offset) <= offsetLimit(state.metric)
            )
        case CustomSensorType.TimeAverage:
        case CustomSensorType.ExponentialMovingAvg:
            return (
                state.hasSingleSource &&
                state.timeWindowSeconds != null &&
                state.timeWindowSeconds >= TIME_WINDOW_SECONDS_MIN &&
                state.timeWindowSeconds <= TIME_WINDOW_SECONDS_MAX
            )
        default:
            return false
    }
}
