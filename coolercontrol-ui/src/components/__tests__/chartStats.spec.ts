// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Dashboard.ts carries class-transformer decorators, which need the polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import {
    chartValueToDisplay,
    emptyChannelStats,
    formatJitterValue,
    formatSpan,
    lineDash,
    windowStats,
    foldChannelStats,
    foldStatusIntoStats,
    formatStatValue,
    isSyntheticStatus,
    lifetimeStatsOf,
    lifetimeToDisplay,
} from '@/components/chartStats.ts'
import { ChannelStatus, Status, TempStatus } from '@/models/Status.ts'
import { DataType } from '@/models/Dashboard.ts'
import { defaultStatsResponse } from '@/models/Stats.ts'

const status = (temps: TempStatus[], channels: ChannelStatus[]): Status =>
    new Status('2026-09-28T12:00:00Z', temps, channels)

describe('isSyntheticStatus', () => {
    it('recognises the daemon zero-fill and nothing else', () => {
        // The daemon keeps the Some/None shape and zeroes every present value.
        const zeroFill = status(
            [new TempStatus('temp1', 0)],
            [
                new ChannelStatus('fan1', 0, 0),
                new ChannelStatus('CPU Freq', undefined, undefined, 0),
            ],
        )
        const stoppedFanWithTemp = status(
            [new TempStatus('temp1', 41.5)],
            [new ChannelStatus('fan1', 0, 0)],
        )
        const zeroRpmFanSpinningDuty = status([], [new ChannelStatus('fan1', 0, 30)])

        expect(isSyntheticStatus(zeroFill)).toBe(true)
        expect(isSyntheticStatus(stoppedFanWithTemp)).toBe(false)
        expect(isSyntheticStatus(zeroRpmFanSpinningDuty)).toBe(false)
    })
})

describe('foldChannelStats', () => {
    it('matches the daemon fold: seeds, tracks extremes, cumulative average, skips NaN', () => {
        const stats = emptyChannelStats()
        foldChannelStats(stats, 40)
        expect(stats).toEqual({ min: 40, max: 40, avg: 40, count: 1 })
        foldChannelStats(stats, Number.NaN)
        expect(stats.count).toBe(1)
        foldChannelStats(stats, 50)
        foldChannelStats(stats, 30)
        expect(stats.min).toBe(30)
        expect(stats.max).toBe(50)
        expect(stats.avg).toBeCloseTo(40)
        expect(stats.count).toBe(3)
    })
})

describe('foldStatusIntoStats and lifetimeStatsOf', () => {
    it('folds temps and every channel field, and reads load back from DUTY', () => {
        const dto = defaultStatsResponse()
        foldStatusIntoStats(
            dto,
            'dev1',
            status(
                [new TempStatus('temp1', 45)],
                [new ChannelStatus('fan1', 900, 40), new ChannelStatus('CPU Load', undefined, 12)],
            ),
        )
        foldStatusIntoStats(
            dto,
            'dev1',
            status([new TempStatus('temp1', 55)], [new ChannelStatus('fan1', 1100, 60)]),
        )

        expect(lifetimeStatsOf(dto, 'dev1', 'temp1', DataType.TEMP)?.avg).toBe(50)
        expect(lifetimeStatsOf(dto, 'dev1', 'fan1', DataType.RPM)?.max).toBe(1100)
        expect(lifetimeStatsOf(dto, 'dev1', 'fan1', DataType.DUTY)?.min).toBe(40)
        expect(lifetimeStatsOf(dto, 'dev1', 'CPU Load', DataType.LOAD)?.count).toBe(1)
        expect(lifetimeStatsOf(dto, 'dev1', 'fan1', DataType.WATTS)).toBeUndefined()
        expect(lifetimeStatsOf(dto, 'missing', 'temp1', DataType.TEMP)).toBeUndefined()
    })
})

describe('display conversion and formatting', () => {
    it('scales only frequencies by precision and hides unobserved stats', () => {
        const freq = { min: 1000, max: 5000, avg: 3000, count: 4 }
        expect(lifetimeToDisplay(freq, DataType.FREQ, 1000)).toEqual({
            min: 1,
            max: 5,
            avg: 3,
            count: 4,
        })
        expect(lifetimeToDisplay(freq, DataType.RPM, 1000)?.max).toBe(5000)
        expect(lifetimeToDisplay(emptyChannelStats(), DataType.TEMP, 1)).toBeNull()
        expect(lifetimeToDisplay(undefined, DataType.TEMP, 1)).toBeNull()
    })

    it('formats like the Table view', () => {
        expect(formatStatValue(45.26, DataType.TEMP, 1)).toBe('45.3')
        expect(formatStatValue(1234.4, DataType.RPM, 1000)).toBe('1234')
        expect(formatStatValue(4.2567, DataType.FREQ, 1000)).toBe('4.26')
        expect(formatStatValue(4256.7, DataType.FREQ, 1)).toBe('4257')
    })
})

describe('windowStats', () => {
    // Five one-second samples; the first is the zero-padded slot of a short history.
    const time = Float64Array.from([0, 101, 102, 103, 104])
    const values = Float32Array.from([0, 40, 0, 60, 50])
    // Index 2 is the daemon zero-fill, so it must not pull the minimum down to 0.
    const valid = Uint8Array.from([0, 1, 0, 1, 1])

    it('counts only real readings inside the window', () => {
        // Jitter pairs only 60 and 50: the zero-fill between 40 and 60 breaks that pair.
        expect(windowStats(time, values, valid, 0, 200)).toEqual({
            min: 40,
            max: 60,
            avg: 50,
            count: 3,
            jitter: 10,
        })
    })

    it('narrows to a zoomed range and reports an empty one as null', () => {
        expect(windowStats(time, values, valid, 103, 104)).toEqual({
            min: 50,
            max: 60,
            avg: 55,
            count: 2,
            jitter: 10,
        })
        expect(windowStats(time, values, valid, 105, 110)).toBeNull()
        expect(windowStats(time, values, new Uint8Array(5), 0, 200)).toBeNull()
    })

    it('measures jitter on a sensor that reports in whole steps', () => {
        // A whole-degree sensor flipping between 40 and 41: the median change would be 0.
        const steps = windowStats(
            Float64Array.from([1, 2, 3, 4, 5]),
            Float32Array.from([40, 40, 41, 40, 40]),
            Uint8Array.from([1, 1, 1, 1, 1]),
            0,
            10,
        )
        expect(steps?.jitter).toBe(0.5)
        // One reading has no neighbour to compare with.
        expect(windowStats(time, values, valid, 101, 101)?.jitter).toBeNull()
        expect(formatJitterValue(0.456, DataType.TEMP, 1)).toBe('0.46')
        expect(formatJitterValue(12.4, DataType.RPM, 1)).toBe('12')
        expect(formatJitterValue(1.26, DataType.DUTY, 1)).toBe('1.3')
    })

    it('converts only chart rpm back to raw units', () => {
        expect(chartValueToDisplay(1.2, DataType.RPM, 1000)).toBe(1200)
        expect(chartValueToDisplay(1.2, DataType.FREQ, 1000)).toBe(1.2)
        expect(chartValueToDisplay(45, DataType.TEMP, 1000)).toBe(45)
    })

    it('keeps the chart line dashes and a locale neutral span', () => {
        expect(lineDash('Hwmon_1_fan1_rpm')).toEqual([1, 1])
        expect(lineDash('Hwmon_1_fan1_duty')).toEqual([10, 3, 2, 3])
        expect(lineDash('CPU_1_temp1_temp')).toEqual([])
        expect(formatSpan(100)).toBe('1:40')
        expect(formatSpan(9.6)).toBe('0:10')
    })
})
