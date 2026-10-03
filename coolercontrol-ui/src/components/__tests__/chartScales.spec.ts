// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { tickSteps } from '@/components/chartScales.ts'

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
