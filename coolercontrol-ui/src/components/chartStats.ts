// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { UID } from '@/models/Device.ts'
import type { Status } from '@/models/Status.ts'
import type { ChannelStatField, ChannelStats, StatsResponseDTO } from '@/models/Stats.ts'
import { DataType } from '@/models/Dashboard.ts'

// The daemon pre-fills a device's history with all-zero statuses at startup and after wake, so
// the charts start full width. They are not readings: every present value is exactly 0.
export function isSyntheticStatus(status: Status): boolean {
    for (const temp of status.temps) {
        if (temp.temp !== 0) return false
    }
    for (const channel of status.channels) {
        if ((channel.duty ?? 0) !== 0) return false
        if ((channel.rpm ?? 0) !== 0) return false
        if ((channel.freq ?? 0) !== 0) return false
        if ((channel.watts ?? 0) !== 0) return false
    }
    return true
}

export const emptyChannelStats = (): ChannelStats => ({ min: 0, max: 0, avg: 0, count: 0 })

// Mirrors the daemon's ChannelStats::fold, so values folded here between /stats fetches match
// what the daemon would report.
export function foldChannelStats(stats: ChannelStats, value: number): void {
    if (Number.isNaN(value)) return
    if (stats.count === 0) {
        stats.min = value
        stats.max = value
        stats.avg = value
        stats.count = 1
        return
    }
    if (value < stats.min) stats.min = value
    if (value > stats.max) stats.max = value
    const newCount = stats.count + 1
    stats.avg = (stats.avg * stats.count + value) / newCount
    stats.count = newCount
}

// Where a data type's lifetime stats live. Load is a duty on the wire; temps have their own map.
export function statFieldOf(dataType: DataType): ChannelStatField | null {
    switch (dataType) {
        case DataType.DUTY:
        case DataType.LOAD:
            return 'DUTY'
        case DataType.RPM:
            return 'RPM'
        case DataType.FREQ:
            return 'FREQ'
        case DataType.WATTS:
            return 'WATTS'
        default:
            return null
    }
}

// Folds one device status into the response, as the daemon's record_stats does each tick.
export function foldStatusIntoStats(dto: StatsResponseDTO, deviceUID: UID, status: Status): void {
    let device = dto.devices.find((d) => d.uid === deviceUID)
    if (device == null) {
        device = { uid: deviceUID, temps: {}, channels: {} }
        dto.devices.push(device)
    }
    for (const temp of status.temps) {
        device.temps[temp.name] ??= emptyChannelStats()
        foldChannelStats(device.temps[temp.name], temp.temp)
    }
    for (const channel of status.channels) {
        const fields = (device.channels[channel.name] ??= {})
        if (channel.duty != null)
            foldChannelStats((fields.DUTY ??= emptyChannelStats()), channel.duty)
        if (channel.rpm != null) foldChannelStats((fields.RPM ??= emptyChannelStats()), channel.rpm)
        if (channel.freq != null)
            foldChannelStats((fields.FREQ ??= emptyChannelStats()), channel.freq)
        if (channel.watts != null) {
            foldChannelStats((fields.WATTS ??= emptyChannelStats()), channel.watts)
        }
    }
}

// The daemon's raw lifetime stats for one line, or undefined when it has none yet.
export function lifetimeStatsOf(
    dto: StatsResponseDTO,
    deviceUID: UID,
    channelName: string,
    dataType: DataType,
): ChannelStats | undefined {
    const device = dto.devices.find((d) => d.uid === deviceUID)
    if (device == null) return undefined
    if (dataType === DataType.TEMP) return device.temps[channelName]
    const field = statFieldOf(dataType)
    return field == null ? undefined : device.channels[channelName]?.[field]
}

// Display units follow the Table view: frequencies scale with the precision setting (MHz or
// GHz), everything else is shown raw.
export function toDisplayUnits(value: number, dataType: DataType, precision: number): number {
    return dataType === DataType.FREQ ? value / precision : value
}

// Lifetime stats converted to display units, or null when nothing has been observed.
export function lifetimeToDisplay(
    stats: ChannelStats | undefined,
    dataType: DataType,
    precision: number,
): ChannelStats | null {
    if (stats == null || stats.count === 0) return null
    return {
        min: toDisplayUnits(stats.min, dataType, precision),
        max: toDisplayUnits(stats.max, dataType, precision),
        avg: toDisplayUnits(stats.avg, dataType, precision),
        count: stats.count,
    }
}

export function formatStatValue(value: number, dataType: DataType, precision: number): string {
    if (dataType === DataType.TEMP || dataType === DataType.WATTS) {
        return value.toFixed(1)
    } else if (dataType === DataType.FREQ && precision > 1) {
        return value.toFixed(2)
    }
    return value.toFixed(0)
}

export function statUnitSuffix(
    dataType: DataType,
    precision: number,
    t: (key: string) => string,
): string {
    switch (dataType) {
        case DataType.TEMP:
            return ` ${t('common.tempUnit')}`
        case DataType.RPM:
            return ` ${t('common.rpmAbbr')}`
        case DataType.FREQ:
            return precision === 1 ? ` ${t('common.mhzAbbr')}` : ` ${t('common.ghzAbbr')}`
        case DataType.WATTS:
            return ` ${t('common.wattAbbr')}`
        default:
            return ` ${t('common.percentUnit')}`
    }
}

// ----- window stats: what a time chart currently shows -----

export interface WindowStats {
    min: number
    max: number
    avg: number
    count: number
    // Mean absolute change between consecutive readings: how noisy the line is. Null without a
    // pair of adjacent readings. The mean, not the median: on sensors that report in whole
    // steps most changes are 0, so a median would read 0 while the value keeps flipping.
    jitter: number | null
}

// Stats over the samples inside [tMin, tMax] that hold a real reading (`valid[i] === 1`), or
// null when there are none. The arrays are one chart line and its time row, index aligned.
// Jitter only pairs neighbours that are both readings, so a gap never counts as a jump.
export function windowStats(
    time: ArrayLike<number>,
    values: ArrayLike<number>,
    valid: ArrayLike<number>,
    tMin: number,
    tMax: number,
): WindowStats | null {
    let min = Number.POSITIVE_INFINITY
    let max = Number.NEGATIVE_INFINITY
    let sum = 0
    let count = 0
    let changeSum = 0
    let changeCount = 0
    let previous: number | null = null
    for (let i = 0; i < values.length; i++) {
        const t = time[i]
        if (valid[i] !== 1 || t < tMin || t > tMax) {
            previous = null
            continue
        }
        const value = values[i]
        if (value < min) min = value
        if (value > max) max = value
        sum += value
        count++
        if (previous !== null) {
            changeSum += Math.abs(value - previous)
            changeCount++
        }
        previous = value
    }
    if (count === 0) return null
    return {
        min,
        max,
        avg: sum / count,
        count,
        jitter: changeCount === 0 ? null : changeSum / changeCount,
    }
}

// Jitter is usually a fraction of the value's own resolution, so it gets one more decimal.
export function formatJitterValue(value: number, dataType: DataType, precision: number): string {
    switch (dataType) {
        case DataType.TEMP:
        case DataType.WATTS:
            return value.toFixed(2)
        case DataType.FREQ:
            return precision > 1 ? value.toFixed(3) : value.toFixed(1)
        case DataType.RPM:
            return value.toFixed(0)
        default:
            return value.toFixed(1)
    }
}

// Time charts store rpm divided by the precision setting so it shares an axis with MHz or GHz.
// Stats are shown in Table view units, where rpm is always raw.
export function chartValueToDisplay(value: number, dataType: DataType, precision: number): number {
    return dataType === DataType.RPM ? value * precision : value
}

export interface WindowLineStats {
    lineName: string
    // The line's uPlot series index (1-based: 0 is the time row).
    seriesIndex: number
    deviceUID: UID
    channelName: string
    dataType: DataType
    color: string
    label: string
    // Display units. Null when the newest sample is not a real reading.
    latest: number | null
    stats: WindowStats | null
}

export interface WindowStatsPayload {
    lines: Array<WindowLineStats>
    spanSeconds: number
    // True when the user zoomed in, so the window is the visible range, not the time range.
    zoomed: boolean
    // Each y scale's current [min, max] in chart units, keyed by scale. Tells whether a limit
    // line is on the chart.
    scaleRanges: Record<string, [number, number]>
}

// The dash pattern a time chart line is drawn with, keyed off its line name suffix.
export function lineDash(lineName: string): Array<number> {
    const lineLower = lineName.toLowerCase()
    if (lineLower.endsWith('rpm') || lineLower.endsWith('freq')) {
        return [1, 1]
    } else if (lineLower.endsWith('load') || lineLower.includes('pump')) {
        return [6, 3]
    } else if (lineLower.endsWith('duty')) {
        return [10, 3, 2, 3]
    } else if (lineLower.endsWith('watts')) {
        return [6, 3, 2, 6]
    }
    return []
}

// A span as m:ss, locale neutral.
export function formatSpan(seconds: number): string {
    const whole = Math.max(0, Math.round(seconds))
    const minutes = Math.floor(whole / 60)
    return `${minutes}:${String(whole % 60).padStart(2, '0')}`
}
