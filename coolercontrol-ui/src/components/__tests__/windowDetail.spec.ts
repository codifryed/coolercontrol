// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Dashboard.ts carries class-transformer decorators, which need the polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import {
    channelDetail,
    directionChanges,
    edgeDecimals,
    minBandWidthFor,
    niceBandWidth,
    stallDutyMinOf,
    stalls,
    statusesInWindow,
    stoppedShare,
    STALL_DUTY_MIN,
    timeInRange,
    windowValues,
} from '@/components/windowDetail.ts'
import { ChannelStatus, Status, TempStatus } from '@/models/Status.ts'
import { DataType } from '@/models/Dashboard.ts'
import type { Calibration } from '@/models/Calibration.ts'

const T0 = Date.parse('2026-09-29T12:00:00Z') / 1000
const status = (second: number, temp: number, duty?: number, rpm?: number): Status =>
    new Status(
        new Date((T0 + second) * 1000).toISOString(),
        [new TempStatus('temp1', temp)],
        [new ChannelStatus('fan1', rpm, duty)],
    )
const N = Number.NaN

describe('statusesInWindow', () => {
    it('keeps the statuses inside the window, oldest first', () => {
        const history = [0, 1, 2, 3, 4].map((s) => status(s, 40 + s))
        const window = statusesInWindow(history, T0 + 1, T0 + 3)
        expect(window.map((s) => s.temps[0].temp)).toEqual([41, 42, 43])
        expect(statusesInWindow(history, T0 + 10, T0 + 20)).toEqual([])
    })
})

describe('windowValues', () => {
    it('marks the zero-fill and missing values as NaN', () => {
        const statuses = [status(0, 0, 0, 0), status(1, 40, 30), status(2, 41, 35, 900)]
        expect(Array.from(windowValues(statuses, 'fan1', DataType.RPM))).toEqual([N, N, 900])
        expect(Array.from(windowValues(statuses, 'fan1', DataType.DUTY))).toEqual([N, 30, 35])
        expect(Array.from(windowValues(statuses, 'fan1', DataType.LOAD))).toEqual([N, 30, 35])
        expect(Array.from(windowValues(statuses, 'temp1', DataType.TEMP))).toEqual([N, 40, 41])
        expect(Array.from(windowValues(statuses, 'missing', DataType.TEMP))).toEqual([N, N, N])
    })
})

describe('directionChanges', () => {
    it('counts turns between rising and falling', () => {
        expect(directionChanges([30, 35, 40, 35, 30, 35])).toBe(2)
        expect(directionChanges([30, 35, 40, 45])).toBe(0)
        expect(directionChanges([])).toBe(0)
    })

    it('skips flat stretches without resetting the direction', () => {
        expect(directionChanges([30, 35, 35, 35, 40])).toBe(0)
        expect(directionChanges([30, 35, 35, 30])).toBe(1)
    })

    it('does not pair readings across a gap', () => {
        expect(directionChanges([30, 35, N, 30, 25])).toBe(0)
    })
})

describe('stoppedShare', () => {
    it('is the share of readings at 0 rpm', () => {
        expect(stoppedShare([0, 0, 900, 950])).toBe(0.5)
        expect(stoppedShare([0, N, N, 900])).toBe(0.5)
        expect(stoppedShare([N, N])).toBeNull()
    })
})

describe('stalls', () => {
    const run = (length: number, value: number): Array<number> => new Array(length).fill(value)

    it('counts 0 rpm runs of 5 polls or more while the duty is high enough', () => {
        const duty = [...run(8, 40), ...run(3, 40), ...run(6, 40)]
        const rpm = [900, ...run(6, 0), 900, ...run(3, 0), 900, ...run(5, 0)]
        // Runs: 6 polls (a stall), 3 polls (too short), 5 polls (a stall).
        expect(stalls(duty, rpm, 30, 1)).toEqual({ count: 2, seconds: 11 })
        expect(stalls(duty, rpm, 30, 2)).toEqual({ count: 2, seconds: 22 })
    })

    it('ignores a fan stopped at low duty, such as a zero RPM mode', () => {
        expect(stalls(run(10, 0), run(10, 0), 30, 1)).toEqual({ count: 0, seconds: 0 })
        expect(stalls(run(10, 20), run(10, 0), 30, 1)).toEqual({ count: 0, seconds: 0 })
    })

    it('pairs each speed reading with the duty set a poll earlier', () => {
        // The duty drops to 0 at index 5; the fan still read 0 at index 5 under the old duty.
        const duty = [...run(5, 40), ...run(5, 0)]
        const rpm = [900, ...run(9, 0)]
        expect(stalls(duty, rpm, 30, 1)).toEqual({ count: 1, seconds: 5 })
        // Spin-up: the duty rises at index 0, the rpm follows a poll later.
        expect(stalls([40, 40, 40], [0, 900, 900], 30, 1)).toEqual({ count: 0, seconds: 0 })
    })

    it('breaks a run at a gap and is null without a duty', () => {
        const duty = run(12, 40)
        const rpm = [900, 0, 0, 0, N, 0, 0, 0, 0, 900, 900, 900]
        expect(stalls(duty, rpm, 30, 1)).toEqual({ count: 0, seconds: 0 })
        expect(stalls(run(8, N), run(8, 0), 30, 1)).toBeNull()
    })
})

describe('stallDutyMinOf', () => {
    const calibration = (curveKind: 'Smooth' | 'Stepped', minStart: number): Calibration =>
        ({ curve_kind: curveKind, min_start_duty: minStart }) as Calibration

    it('uses the calibrated start duty where it is known', () => {
        expect(stallDutyMinOf(undefined)).toBe(STALL_DUTY_MIN)
        expect(stallDutyMinOf(calibration('Smooth', 25))).toBe(1)
        expect(stallDutyMinOf(calibration('Stepped', 25))).toBe(25)
        expect(stallDutyMinOf(calibration('Stepped', 0))).toBe(1)
    })
})

describe('band widths', () => {
    it('rounds a tenth of the span to the nearest 1-2-5 step', () => {
        expect(niceBandWidth(40, 1)).toBe(5)
        expect(niceBandWidth(12, 1)).toBe(1)
        expect(niceBandWidth(2500, 10)).toBe(200)
        expect(niceBandWidth(0, 1)).toBe(1)
        expect(niceBandWidth(0.4, 0.01)).toBeCloseTo(0.05)
    })

    it('never goes below what the sensor resolves', () => {
        expect(minBandWidthFor(DataType.TEMP, 1)).toBe(1)
        expect(minBandWidthFor(DataType.RPM, 1000)).toBe(10)
        expect(minBandWidthFor(DataType.FREQ, 1)).toBe(10)
        expect(minBandWidthFor(DataType.FREQ, 1000)).toBe(0.01)
    })

    it('prints as many decimals as the step has', () => {
        expect(edgeDecimals(5)).toBe(0)
        expect(edgeDecimals(0.5)).toBe(1)
        expect(edgeDecimals(0.05)).toBe(2)
    })
})

describe('timeInRange', () => {
    it('keeps the five most visited bands and the rest', () => {
        // 1 °C bands from 40 to 46; 46 holds the most readings.
        const values = [40, 41, 42, 43, 44, 45, 45, 46, 46, 46]
        const result = timeInRange(values, 1, 46.4)!
        expect(result.width).toBe(1)
        expect(result.bands.map((b) => b.from)).toEqual([46, 45, 40, 41, 42])
        expect(result.bands[0]).toEqual({ from: 46, to: 47, share: 0.3, current: true })
        expect(result.bands.filter((b) => b.current)).toHaveLength(1)
        expect(result.rest).toBeCloseTo(0.2)
    })

    it('skips missing readings and flags no band for a value outside the window', () => {
        const result = timeInRange([N, 50, 50, N, 51], 1, 70)!
        expect(result.bands.map((b) => [b.from, b.share])).toEqual([
            [50, 2 / 3],
            [51, 1 / 3],
        ])
        expect(result.bands.some((b) => b.current)).toBe(false)
        expect(result.rest).toBe(0)
        expect(timeInRange([N, N], 1, null)).toBeNull()
    })

    it('puts a reading on a decimal edge in the band it starts', () => {
        const result = timeInRange([0.7, 0.8, 1.6], 0.01, null)!
        expect(result.width).toBeCloseTo(0.1)
        expect(result.bands.find((b) => Math.abs(b.from - 0.7) < 1e-9)?.share).toBeCloseTo(1 / 3)
    })
})

describe('channelDetail', () => {
    const options = { precision: 1, pollSeconds: 1, stallDutyMin: 30 }

    it('fills the rows each line type has', () => {
        const history = [
            status(0, 0, 0, 0),
            status(1, 40, 30, 900),
            status(2, 41, 35, 950),
            status(3, 42, 30, 900),
        ]
        const detail = channelDetail(
            history,
            'fan1',
            [DataType.DUTY, DataType.RPM],
            T0,
            T0 + 3,
            options,
        )
        const duty = detail.get(DataType.DUTY)!
        expect(duty.directionChanges).toBe(1)
        expect(duty.directionChangesPerMinute).toBeCloseTo(20)
        expect(duty.stoppedShare).toBeNull()
        expect(duty.timeInRange?.bands.find((b) => b.current)?.from).toBe(30)
        const rpm = detail.get(DataType.RPM)!
        expect(rpm.directionChanges).toBeNull()
        expect(rpm.stoppedShare).toBe(0)
        expect(rpm.stalls).toEqual({ count: 0, seconds: 0 })

        const temp = channelDetail(history, 'temp1', [DataType.TEMP], T0, T0 + 3, options)
        expect(temp.get(DataType.TEMP)?.stalls).toBeNull()
        expect(temp.get(DataType.TEMP)?.timeInRange?.bands).toHaveLength(3)
    })

    it('bins frequencies in display units', () => {
        const history = [4200, 4250, 4800].map(
            (freq, s) =>
                new Status(
                    new Date((T0 + s) * 1000).toISOString(),
                    [],
                    [new ChannelStatus('CPU Freq', undefined, undefined, freq)],
                ),
        )
        const detail = channelDetail(history, 'CPU Freq', [DataType.FREQ], T0, T0 + 2, {
            ...options,
            precision: 1000,
        })
        const result = detail.get(DataType.FREQ)!.timeInRange!
        expect(result.width).toBeCloseTo(0.05)
        expect(result.bands.find((b) => b.current)?.from).toBeCloseTo(4.8)
    })
})
