// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Dashboard.ts carries class-transformer decorators, which need the polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import {
    emptyChannelStats,
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
