// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Dashboard model's decorators need it loaded first.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import {
    applyBlock,
    metricDataType,
    metricHasDriverLimits,
    observedRange,
    runReachesWarmup,
    thresholdAttributes,
    timeOutside,
} from '@/components/alertReference.ts'
import { ChannelMetric } from '@/models/ChannelSource.ts'
import { DataType } from '@/models/Dashboard.ts'
import type { ChannelAttribute } from '@/models/ChannelAttributes.ts'

const stats = (min: number, max: number) => ({ min, max, avg: (min + max) / 2, count: 10 })

describe('thresholdAttributes', () => {
    const temp: Array<ChannelAttribute> = [
        { name: 'temp1_max', kind: 'TEMP_MAX', value: 89.85 },
        { name: 'temp1_crit', kind: 'TEMP_CRIT', value: 94.85 },
        { name: 'temp1_crit_hyst', kind: 'TEMP_CRIT_HYST', value: 92 },
        { name: 'temp1_offset', kind: 'TEMP_OFFSET', value: 2 },
        { name: 'temp1_type', kind: 'TEMP_TYPE', value: 3 },
    ]
    // The Leakshield's pressure limits from issue #612.
    const fan: Array<ChannelAttribute> = [
        { name: 'fan1_min', kind: 'FAN_MIN', value: 382_800 },
        { name: 'fan1_max', kind: 'FAN_MAX', value: 472_800 },
        { name: 'fan1_div', kind: 'FAN_DIV', value: 2 },
        { name: 'fan1_pulses', kind: 'FAN_PULSES', value: 2 },
    ]

    it('keeps temperature limits and drops sensor details', () => {
        expect(thresholdAttributes(temp, ChannelMetric.Temp).map((a) => a.name)).toEqual([
            'temp1_max',
            'temp1_crit',
            'temp1_crit_hyst',
        ])
    })

    it('keeps fan limits for an rpm alert and drops divisor and pulses', () => {
        expect(thresholdAttributes(fan, ChannelMetric.RPM).map((a) => a.name)).toEqual([
            'fan1_min',
            'fan1_max',
        ])
    })

    it('offers nothing in another unit', () => {
        // A fan's rpm limits say nothing about its duty.
        expect(thresholdAttributes(fan, ChannelMetric.Duty)).toEqual([])
        expect(thresholdAttributes(fan, ChannelMetric.Temp)).toEqual([])
        expect(thresholdAttributes(temp, ChannelMetric.RPM)).toEqual([])
        expect(thresholdAttributes(temp, undefined)).toEqual([])
    })

    it('knows which metrics a driver can report limits for', () => {
        expect(metricHasDriverLimits(ChannelMetric.Temp)).toBe(true)
        expect(metricHasDriverLimits(ChannelMetric.RPM)).toBe(true)
        expect(metricHasDriverLimits(ChannelMetric.Duty)).toBe(false)
        expect(metricHasDriverLimits(ChannelMetric.Load)).toBe(false)
        expect(metricHasDriverLimits(undefined)).toBe(false)
    })
})

describe('metricDataType', () => {
    it('maps each alert metric to its stats data type', () => {
        expect(metricDataType(ChannelMetric.Temp)).toBe(DataType.TEMP)
        expect(metricDataType(ChannelMetric.Duty)).toBe(DataType.DUTY)
        expect(metricDataType(ChannelMetric.Load)).toBe(DataType.LOAD)
        expect(metricDataType(ChannelMetric.RPM)).toBe(DataType.RPM)
    })
})

describe('observedRange', () => {
    it('spans the lowest min and the highest max', () => {
        expect(observedRange([stats(480, 900), stats(650, 2890), stats(500, 1200)])).toEqual({
            min: 480,
            max: 2890,
        })
    })

    it('skips sources with no readings yet', () => {
        expect(observedRange([null, stats(30, 60), undefined])).toEqual({ min: 30, max: 60 })
        expect(observedRange([null, undefined])).toBeNull()
        expect(observedRange([])).toBeNull()
    })
})

describe('applyBlock', () => {
    const thresholds = { min: 100, max: 10_000, ceiling: 9_999_999 }

    it('allows a Greater Than above the current Less Than', () => {
        expect(applyBlock('max', 472_800, thresholds)).toBeNull()
        expect(applyBlock('max', 101, thresholds)).toBeNull()
    })

    it('blocks a Greater Than at or below the current Less Than', () => {
        expect(applyBlock('max', 100, thresholds)).toBe('crossesOther')
        expect(applyBlock('max', 0, thresholds)).toBe('crossesOther')
    })

    it('blocks a Greater Than above what the editor can hold', () => {
        expect(applyBlock('max', 10_000_000, thresholds)).toBe('outsideRange')
        expect(applyBlock('max', 250, { min: 0, max: 100, ceiling: 200 })).toBe('outsideRange')
    })

    it('allows a Less Than below the current Greater Than', () => {
        expect(applyBlock('min', 0, thresholds)).toBeNull()
        expect(applyBlock('min', 9999, thresholds)).toBeNull()
    })

    it('blocks a Less Than at or above the current Greater Than', () => {
        expect(applyBlock('min', 10_000, thresholds)).toBe('crossesOther')
        expect(applyBlock('min', 382_800, thresholds)).toBe('crossesOther')
    })

    it('blocks a negative or non-finite value', () => {
        // nvme reports a real temp1_min of -5.15, and thresholds cannot be negative.
        expect(applyBlock('min', -5.15, thresholds)).toBe('outsideRange')
        expect(applyBlock('max', Number.NaN, thresholds)).toBe('outsideRange')
    })
})

describe('timeOutside', () => {
    const N = Number.NaN

    it('counts readings above and below, and the longest run', () => {
        //             in   above above in  below in  above above above
        const values = [50, 90, 95, 60, 10, 55, 81, 99, 85]
        expect(timeOutside(values, 20, 80)).toEqual({
            readings: 9,
            above: 5,
            below: 1,
            longestRun: 3,
        })
    })

    it('treats a reading on a threshold as in range, like the daemon', () => {
        expect(timeOutside([20, 80, 20, 80], 20, 80)).toEqual({
            readings: 4,
            above: 0,
            below: 0,
            longestRun: 0,
        })
    })

    it('joins above and below into one run when nothing in between is in range', () => {
        // The daemon's source stays out of range across the jump.
        expect(timeOutside([90, 10, 90], 20, 80)?.longestRun).toBe(3)
    })

    it('does not count a poll without a reading, and lets it end a run', () => {
        expect(timeOutside([90, 90, N, 90, 90, 90, N], 20, 80)).toEqual({
            readings: 5,
            above: 5,
            below: 0,
            longestRun: 3,
        })
    })

    it('never reports below with a zero lower threshold', () => {
        expect(timeOutside([0, 0, 1200, 0], 0, 10_000)?.below).toBe(0)
    })

    it('is null without readings', () => {
        expect(timeOutside([], 20, 80)).toBeNull()
        expect(timeOutside([N, N], 20, 80)).toBeNull()
    })

    it('reads a typed array', () => {
        expect(timeOutside(Float64Array.from([438_300, N, 480_000]), 382_800, 472_800)).toEqual({
            readings: 2,
            above: 1,
            below: 0,
            longestRun: 1,
        })
    })
})

describe('runReachesWarmup', () => {
    it('never triggers on a single reading, as the daemon needs two', () => {
        expect(runReachesWarmup(0, 1, 0)).toBe(false)
        expect(runReachesWarmup(1, 1, 0)).toBe(false)
        expect(runReachesWarmup(1, 5, 1)).toBe(false)
    })

    it('triggers on the second reading with no warmup', () => {
        expect(runReachesWarmup(2, 1, 0)).toBe(true)
    })

    it('measures the time from the first reading of the run', () => {
        // Three readings at a 1 s poll span 2 s.
        expect(runReachesWarmup(3, 1, 1.5)).toBe(true)
        expect(runReachesWarmup(3, 1, 2.5)).toBe(false)
        expect(runReachesWarmup(12, 0.5, 5)).toBe(true)
        expect(runReachesWarmup(10, 0.5, 5)).toBe(false)
    })

    it('does not count a run that only equals the warmup', () => {
        // The daemon's ticks are not exactly a poll apart, so it may measure just under.
        expect(runReachesWarmup(2, 1, 1)).toBe(false)
        expect(runReachesWarmup(3, 1, 2)).toBe(false)
        expect(runReachesWarmup(2, 5, 5)).toBe(false)
        expect(runReachesWarmup(11, 0.5, 5)).toBe(false)
        expect(runReachesWarmup(3, 1, 1)).toBe(true)
    })
})
