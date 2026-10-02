// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { ChannelMetric } from '@/models/ChannelSource.ts'
import { DataType } from '@/models/Dashboard.ts'
import type { ChannelAttribute, ChannelAttributeKind } from '@/models/ChannelAttributes.ts'
import type { ChannelStats } from '@/models/Stats.ts'
import { THRESHOLD_GAP } from '@/components/alertThresholds.ts'

// One selected alert source, as the alert editor lists it.
export interface ReferenceSource {
    deviceUID: string
    channelName: string
    channelFrontendName: string
    lineColor: string
    // The live reading, already formatted.
    value: string
    metric: ChannelMetric
}

export const metricDataType = (metric: ChannelMetric): DataType => {
    switch (metric) {
        case ChannelMetric.Duty:
            return DataType.DUTY
        case ChannelMetric.Load:
            return DataType.LOAD
        case ChannelMetric.RPM:
            return DataType.RPM
        case ChannelMetric.Freq:
            return DataType.FREQ
        case ChannelMetric.Temp:
        default:
            return DataType.TEMP
    }
}

const metricAttributePrefix = (metric: ChannelMetric | undefined): string | undefined => {
    if (metric === ChannelMetric.Temp) return 'TEMP_'
    if (metric === ChannelMetric.RPM) return 'FAN_'
    return undefined
}

// Whether a driver can report limits in this metric's unit at all. Duty and load have none,
// so their sources are never read.
export const metricHasDriverLimits = (metric: ChannelMetric | undefined): boolean =>
    metricAttributePrefix(metric) != null

// Sensor details that are not in the alert's unit, so they cannot be a threshold.
const NOT_A_THRESHOLD: ReadonlySet<ChannelAttributeKind> = new Set([
    'TEMP_OFFSET',
    'TEMP_TYPE',
    'FAN_DIV',
    'FAN_PULSES',
])

// The driver attributes that could serve as a threshold of an alert on this metric.
export const thresholdAttributes = (
    attributes: ReadonlyArray<ChannelAttribute>,
    metric: ChannelMetric | undefined,
): Array<ChannelAttribute> => {
    const prefix = metricAttributePrefix(metric)
    if (prefix == null) return []
    return attributes.filter(
        (attribute) => attribute.kind.startsWith(prefix) && !NOT_A_THRESHOLD.has(attribute.kind),
    )
}

export interface ObservedRange {
    min: number
    max: number
}

// The range that holds every source's readings: thresholds taken from it fit all of them.
export const observedRange = (
    stats: ReadonlyArray<ChannelStats | null | undefined>,
): ObservedRange | null => {
    let range: ObservedRange | null = null
    for (const stat of stats) {
        if (stat == null) continue
        if (range == null) {
            range = { min: stat.min, max: stat.max }
            continue
        }
        range.min = Math.min(range.min, stat.min)
        range.max = Math.max(range.max, stat.max)
    }
    return range
}

// 'max' is the editor's "Greater Than", 'min' its "Less Than".
export type ThresholdTarget = 'max' | 'min'
export type ApplyBlock = 'crossesOther' | 'outsideRange'

export interface Thresholds {
    min: number
    max: number
    // The highest "Greater Than" the editor can hold once unlocked.
    ceiling: number
}

// Why a value cannot become a threshold, or null when it can. Applying must never move the
// other threshold, which the user did not click.
export const applyBlock = (
    target: ThresholdTarget,
    value: number,
    thresholds: Thresholds,
): ApplyBlock | null => {
    if (!Number.isFinite(value)) return 'outsideRange'
    if (target === 'max') {
        if (value > thresholds.ceiling) return 'outsideRange'
        return value >= thresholds.min + THRESHOLD_GAP ? null : 'crossesOther'
    }
    if (value < 0) return 'outsideRange'
    return value <= thresholds.max - THRESHOLD_GAP ? null : 'crossesOther'
}

// How one source's recorded readings sit against a pair of thresholds. Counts are readings,
// one per poll.
export interface TimeOutside {
    readings: number
    above: number
    below: number
    // The most consecutive readings out of range, on either side.
    longestRun: number
}

// Measures the readings against the thresholds the way the daemon does: in range means
// min <= value <= max. A NaN is a poll without a reading: it is not counted and it ends a
// run. Null when there are no readings.
export function timeOutside(
    values: ArrayLike<number>,
    min: number,
    max: number,
): TimeOutside | null {
    const result: TimeOutside = { readings: 0, above: 0, below: 0, longestRun: 0 }
    let run = 0
    for (let i = 0; i < values.length; i++) {
        const value = values[i]
        if (Number.isNaN(value)) {
            run = 0
            continue
        }
        result.readings++
        if (value > max) {
            result.above++
        } else if (value < min) {
            result.below++
        } else {
            run = 0
            continue
        }
        run++
        if (run > result.longestRun) result.longestRun = run
    }
    return result.readings === 0 ? null : result
}

// Whether a run of readings out of range would have triggered an alert. The daemon starts
// its warmup clock on the first of them and triggers on a later one once the warmup has
// passed, so a single reading never triggers, whatever the warmup.
export const runReachesWarmup = (
    run: number,
    pollSeconds: number,
    warmupSeconds: number,
): boolean => run >= 2 && (run - 1) * pollSeconds >= warmupSeconds
