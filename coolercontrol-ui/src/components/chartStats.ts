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
