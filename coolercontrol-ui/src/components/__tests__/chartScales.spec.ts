// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { sharedAxisUnit, tickSteps, unitTickSteps } from '@/components/chartScales.ts'

describe('tickSteps', () => {
    it('gives the five steps of one decade for an ordinary fan scale', () => {
        expect(tickSteps(1000, 8000)).toEqual([1000, 2000, 2500, 5000, 10_000])
        expect(tickSteps(1, 6)).toEqual([1, 2, 2.5, 5, 10])
    })

    it('reaches past the top of a large scale', () => {
        // The Leakshield's pressure from issue #612, where the axis lost its ticks.
        expect(tickSteps(1000, 480_000)).toEqual([
            1000, 2000, 2500, 5000, 10_000, 20_000, 25_000, 50_000, 100_000, 200_000, 250_000,
            500_000, 1_000_000,
        ])
        // The same scale in krpm.
        expect(tickSteps(1, 480).at(-1)).toBe(1000)
    })

    it('rises in order, as uPlot searches it', () => {
        const steps = tickSteps(1000, 4_294_967_295)
        expect(steps).toEqual([...steps].sort((a, b) => a - b))
        expect(steps.at(-1)).toBeGreaterThan(4_294_967_295)
    })

    it('still ends at the next decade when the scale is below the base', () => {
        expect(tickSteps(1000, 0)).toEqual([1000, 2000, 2500, 5000, 10_000])
    })

    it('is bounded for a scale with no top', () => {
        expect(tickSteps(1, Number.POSITIVE_INFINITY).length).toBe(12 * 4 + 1)
    })
})

describe('unitTickSteps', () => {
    it('starts at the step the fan axis uses for a fan-sized scale', () => {
        expect(unitTickSteps(1000)[0]).toBe(100)
        expect(unitTickSteps(1500)[0]).toBe(200)
        expect(unitTickSteps(5000)[0]).toBe(500)
    })

    it('keeps ticks on a scale far below a fan speed', () => {
        expect(unitTickSteps(110)[0]).toBe(20)
        expect(unitTickSteps(12)[0]).toBe(2)
        expect(unitTickSteps(1.1)).toEqual([1, 2, 5, 10])
    })

    it('reaches past the top of a large scale', () => {
        const steps = unitTickSteps(480_000)
        expect(steps[0]).toBe(50_000)
        expect(steps.at(-1)).toBe(1_000_000)
    })

    it('offers whole steps only, as the readings are whole', () => {
        for (const scaleMax of [0, 3, 22, 480, 65_535]) {
            expect(unitTickSteps(scaleMax).every(Number.isInteger)).toBe(true)
        }
    })

    it('never starts with more than about ten ticks', () => {
        for (const scaleMax of [7, 99, 1234, 30_000, 3_276_700]) {
            expect(scaleMax / unitTickSteps(scaleMax)[0]).toBeLessThanOrEqual(10)
        }
    })

    it('stays on the steps uPlot draws once scaled to krpm', () => {
        // uPlot skips a fractional step that is not one of its own, which it spells this way.
        const drawn = new Set<number>()
        for (let exponent = -3; exponent <= 9; exponent++) {
            for (const multiplier of [1, 2, 2.5, 5]) drawn.add(Number(`${multiplier}e${exponent}`))
        }
        for (const scaleMax of [3, 110, 1500, 480_000]) {
            for (const step of unitTickSteps(scaleMax)) {
                expect(drawn.has(step / 1000)).toBe(true)
            }
        }
    })
})

describe('sharedAxisUnit', () => {
    it('is the unit every line names', () => {
        expect(sharedAxisUnit(['dL/h'])).toBe('dL/h')
        expect(sharedAxisUnit(['dL/h', 'dL/h'])).toBe('dL/h')
    })

    it('is none when a line is in rpm or MHz', () => {
        expect(sharedAxisUnit([undefined])).toBeUndefined()
        expect(sharedAxisUnit(['dL/h', undefined])).toBeUndefined()
        expect(sharedAxisUnit([undefined, 'dL/h'])).toBeUndefined()
    })

    it('is none when the lines name different units', () => {
        expect(sharedAxisUnit(['dL/h', 'mbar'])).toBeUndefined()
        // Units are shown as written, so a different spelling is a different title.
        expect(sharedAxisUnit(['dL/h', 'dl/h'])).toBeUndefined()
    })

    it('is none for an axis with no lines', () => {
        expect(sharedAxisUnit([])).toBeUndefined()
    })
})
