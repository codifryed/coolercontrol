// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { Status } from '@/models/Status.ts'
import type { Calibration } from '@/models/Calibration.ts'
import { DataType } from '@/models/Dashboard.ts'
import { isSyntheticStatus, toDisplayUnits } from '@/components/chartStats.ts'

// The stats panel's measured behaviour rows: time in range for every line, direction changes
// for duty, stopped time and stalls for speed. All of them cover the chart's current window.

export const TOP_BANDS = 5
const TARGET_BANDS = 10
// Rounding to the nearest 1-2-5 step can shrink a band to 1/1.5 of span / TARGET_BANDS, so a
// window never needs more than 16 bands; this bounds a bad input all the same.
const MAX_BANDS = 24
// Band edges are multiples of a decimal step, which binary floats miss by a hair (0.7 / 0.1 is
// 6.999...). Without the nudge a reading on an edge lands in the band below.
const EDGE_EPSILON = 1e-9

export const STALL_DUTY_MIN = 30
export const STALL_POLLS_MIN = 5

export interface TimeInRangeBand {
    // Display units. The band is [from, to).
    from: number
    to: number
    share: number
    // Holds the channel's current value.
    current: boolean
}

export interface TimeInRange {
    // The most visited bands, most visited first.
    bands: Array<TimeInRangeBand>
    // Share of the window spent in all other bands.
    rest: number
    width: number
}

export interface StallCount {
    count: number
    seconds: number
}

export interface LineDetail {
    // Duty lines only.
    directionChanges: number | null
    directionChangesPerMinute: number | null
    // Speed lines only.
    stoppedShare: number | null
    // Speed lines of channels that also report a duty.
    stalls: StallCount | null
    timeInRange: TimeInRange | null
}

export interface DetailOptions {
    precision: number
    pollSeconds: number
    stallDutyMin: number
}

// The statuses whose timestamps (in seconds) fall inside [tMin, tMax], oldest first. History is
// in time order, so the scan runs from the newest status and stops at the first one before tMin.
export function statusesInWindow(
    history: ReadonlyArray<Status>,
    tMin: number,
    tMax: number,
): Array<Status> {
    const statuses: Array<Status> = []
    for (let i = history.length - 1; i >= 0; i--) {
        const time = new Date(history[i].timestamp).getTime() / 1000
        if (time < tMin) break
        if (time <= tMax) statuses.push(history[i])
    }
    return statuses.reverse()
}

function valueOf(status: Status, channelName: string, dataType: DataType): number | undefined {
    if (dataType === DataType.TEMP) {
        return status.temps.find((temp) => temp.name === channelName)?.temp
    }
    const channel = status.channels.find((c) => c.name === channelName)
    switch (dataType) {
        case DataType.DUTY:
        case DataType.LOAD:
            return channel?.duty
        case DataType.RPM:
            return channel?.rpm
        case DataType.FREQ:
            return channel?.freq
        case DataType.WATTS:
            return channel?.watts
        default:
            return undefined
    }
}

// One line's raw values across the statuses, NaN where there is no reading: the daemon's
// zero-fill, or a status without the value. A NaN breaks every pair that would span it.
export function windowValues(
    statuses: ReadonlyArray<Status>,
    channelName: string,
    dataType: DataType,
): Float64Array {
    const values = new Float64Array(statuses.length)
    for (const [index, status] of statuses.entries()) {
        const value = isSyntheticStatus(status) ? undefined : valueOf(status, channelName, dataType)
        values[index] = value ?? Number.NaN
    }
    return values
}

// How often the line turned from rising to falling or back. Flat stretches do not reset the
// direction, so a duty that holds between two steps up is not a change.
export function directionChanges(values: ArrayLike<number>): number {
    let changes = 0
    let previous = Number.NaN
    let lastSign = 0
    for (let i = 0; i < values.length; i++) {
        const value = values[i]
        if (Number.isNaN(value)) {
            previous = Number.NaN
            lastSign = 0
            continue
        }
        const delta = value - previous
        if (delta !== 0 && !Number.isNaN(delta)) {
            const sign = Math.sign(delta)
            if (lastSign !== 0 && sign !== lastSign) changes++
            lastSign = sign
        }
        previous = value
    }
    return changes
}

function readingCount(values: ArrayLike<number>): number {
    let count = 0
    for (let i = 0; i < values.length; i++) {
        if (!Number.isNaN(values[i])) count++
    }
    return count
}

// Share of the readings at 0 rpm, or null without readings.
export function stoppedShare(rpm: ArrayLike<number>): number | null {
    let readings = 0
    let stopped = 0
    for (let i = 0; i < rpm.length; i++) {
        if (Number.isNaN(rpm[i])) continue
        readings++
        if (rpm[i] === 0) stopped++
    }
    return readings === 0 ? null : stopped / readings
}

// Runs of at least STALL_POLLS_MIN polls at 0 rpm while the fan was told to spin. A speed
// reading answers the duty set one poll earlier, so each rpm pairs with the previous duty. Null
// when no such pair exists (the channel reports no duty).
export function stalls(
    duty: ArrayLike<number>,
    rpm: ArrayLike<number>,
    dutyMin: number,
    pollSeconds: number,
): StallCount | null {
    let paired = false
    let count = 0
    let stalledPolls = 0
    let run = 0
    const closeRun = (): void => {
        if (run >= STALL_POLLS_MIN) {
            count++
            stalledPolls += run
        }
        run = 0
    }
    for (let i = 1; i < rpm.length; i++) {
        const speed = rpm[i]
        const commanded = duty[i - 1]
        if (Number.isNaN(speed) || Number.isNaN(commanded)) {
            closeRun()
            continue
        }
        paired = true
        if (speed === 0 && commanded >= dutyMin) {
            run++
        } else {
            closeRun()
        }
    }
    closeRun()
    return paired ? { count, seconds: stalledPolls * pollSeconds } : null
}

// The duty from which a fan must be turning. Calibrated smooth channels report true duty, where
// anything above 0 spins (the dispatcher kicks the fan from off). Stepped channels pass the
// device duty through, which starts the fan from its calibrated start duty.
export function stallDutyMinOf(calibration: Calibration | undefined): number {
    if (calibration == null) return STALL_DUTY_MIN
    if (calibration.curve_kind === 'Smooth') return 1
    return Math.max(1, calibration.min_start_duty)
}

// The smallest band that still means something for a value in display units: finer than the
// sensor reports only splits one reading across empty bands.
export function minBandWidthFor(dataType: DataType, precision: number): number {
    switch (dataType) {
        case DataType.RPM:
            return 10
        case DataType.FREQ:
            return precision > 1 ? 0.01 : 10
        default:
            return 1
    }
}

// The 1-2-5 step nearest to span / TARGET_BANDS, never below minWidth.
export function niceBandWidth(span: number, minWidth: number): number {
    const raw = Math.max(span / TARGET_BANDS, minWidth)
    const magnitude = 10 ** Math.floor(Math.log10(raw))
    const fraction = raw / magnitude
    const step = fraction < 1.5 ? 1 : fraction < 3.5 ? 2 : fraction < 7.5 ? 5 : 10
    return Math.max(step * magnitude, minWidth)
}

// Decimals needed to print a band edge: none for whole steps, otherwise as many as the step has.
export function edgeDecimals(width: number): number {
    if (Number.isInteger(width)) return 0
    return Math.min(3, Math.max(0, Math.ceil(-Math.log10(width))))
}

// Counts the readings into bands aligned to multiples of the width, so the edges read as round
// numbers, and keeps the TOP_BANDS most visited. Null without readings.
export function timeInRange(
    values: ArrayLike<number>,
    minWidth: number,
    current: number | null,
): TimeInRange | null {
    let low = Number.POSITIVE_INFINITY
    let high = Number.NEGATIVE_INFINITY
    let total = 0
    for (let i = 0; i < values.length; i++) {
        const value = values[i]
        if (Number.isNaN(value)) continue
        if (value < low) low = value
        if (value > high) high = value
        total++
    }
    if (total === 0) return null
    const width = niceBandWidth(high - low, minWidth)
    const start = Math.floor(low / width + EDGE_EPSILON) * width
    const indexOf = (value: number): number => Math.floor((value - start) / width + EDGE_EPSILON)
    const bandCount = Math.min(MAX_BANDS, indexOf(high) + 1)
    const counts = new Array<number>(bandCount).fill(0)
    for (let i = 0; i < values.length; i++) {
        const value = values[i]
        if (Number.isNaN(value)) continue
        counts[Math.min(bandCount - 1, Math.max(0, indexOf(value)))]++
    }
    const currentIndex = current == null ? -1 : indexOf(current)
    const bands: Array<TimeInRangeBand> = []
    for (const [index, count] of counts.entries()) {
        if (count === 0) continue
        bands.push({
            from: start + index * width,
            to: start + (index + 1) * width,
            share: count / total,
            current: index === currentIndex,
        })
    }
    bands.sort((a, b) => b.share - a.share || a.from - b.from)
    const top = bands.slice(0, TOP_BANDS)
    const topShare = top.reduce((sum, band) => sum + band.share, 0)
    return { bands: top, rest: Math.max(0, 1 - topShare), width }
}

// The newest status's raw value for one line, or null when it holds no reading.
function currentValue(
    history: ReadonlyArray<Status>,
    channelName: string,
    dataType: DataType,
): number | null {
    const newest = history[history.length - 1]
    if (newest == null || isSyntheticStatus(newest)) return null
    return valueOf(newest, channelName, dataType) ?? null
}

// The detail rows for each of one channel's lines over the window [tMin, tMax] (seconds).
export function channelDetail(
    history: ReadonlyArray<Status>,
    channelName: string,
    dataTypes: ReadonlyArray<DataType>,
    tMin: number,
    tMax: number,
    options: DetailOptions,
): Map<DataType, LineDetail> {
    const statuses = statusesInWindow(history, tMin, tMax)
    const details = new Map<DataType, LineDetail>()
    for (const dataType of dataTypes) {
        const values = windowValues(statuses, channelName, dataType)
        const detail: LineDetail = {
            directionChanges: null,
            directionChangesPerMinute: null,
            stoppedShare: null,
            stalls: null,
            timeInRange: null,
        }
        if (dataType === DataType.DUTY) {
            const readings = readingCount(values)
            detail.directionChanges = readings === 0 ? null : directionChanges(values)
            const minutes = (readings * options.pollSeconds) / 60
            detail.directionChangesPerMinute =
                detail.directionChanges == null || readings < 2
                    ? null
                    : detail.directionChanges / minutes
        }
        if (dataType === DataType.RPM) {
            detail.stoppedShare = stoppedShare(values)
            const duty = windowValues(statuses, channelName, DataType.DUTY)
            detail.stalls = stalls(duty, values, options.stallDutyMin, options.pollSeconds)
        }
        for (let i = 0; i < values.length; i++) {
            values[i] = toDisplayUnits(values[i], dataType, options.precision)
        }
        const current = currentValue(history, channelName, dataType)
        detail.timeInRange = timeInRange(
            values,
            minBandWidthFor(dataType, options.precision),
            current == null ? null : toDisplayUnits(current, dataType, options.precision),
        )
        details.set(dataType, detail)
    }
    return details
}
