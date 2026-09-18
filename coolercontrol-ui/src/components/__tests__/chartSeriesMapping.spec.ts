// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { lineDataIndex, lineSetMatches } from '@/components/chartSeriesMapping.ts'

// TimeChart builds its uPlot series once, from the line set it finds at mount, and then only
// replaces the data. Series and data are paired by position, so any drift between the two
// draws one channel's values as another: a 372 rpm reading appeared as "PSU Fan 0.372 %",
// an rpm series (stored as rpm / 1000 in krpm mode) landing on a duty's label and scale.
describe('chart line to data mapping', () => {
    const lineNames = ['CPU_1_CPU Load_load', 'CPU_1_CPU Freq_freq', 'Hwmon_1_PSU Fan_rpm']

    it('skips the timestamp row when addressing a line', () => {
        expect(lineDataIndex(lineNames, 'CPU_1_CPU Load_load')).toBe(1)
        expect(lineDataIndex(lineNames, 'Hwmon_1_PSU Fan_rpm')).toBe(3)
    })

    it('reports a line the chart does not have rather than pointing at the timestamps', () => {
        // The trap this replaces: indexOf returns -1 for an unknown line, and -1 + 1 is the
        // timestamp row, so a channel that started reporting overwrote the whole x axis.
        expect(lineDataIndex(lineNames, 'Hwmon_1_New Fan_rpm')).toBeNull()
        expect(lineDataIndex([], 'CPU_1_CPU Load_load')).toBeNull()
    })

    it('accepts an unchanged line set', () => {
        expect(lineSetMatches(lineNames, [...lineNames])).toBe(true)
        expect(lineSetMatches([], [])).toBe(true)
    })

    it('rejects a set that gained or lost a line', () => {
        // A device that drops out long enough to leave the chart's time window loses its
        // lines, and gets them back when it returns. Both shift every later line by one.
        expect(lineSetMatches(lineNames, lineNames.slice(1))).toBe(false)
        expect(lineSetMatches(lineNames, [...lineNames, 'Hwmon_1_New Fan_rpm'])).toBe(false)
    })

    it('rejects a reordered set, which positions alone cannot detect', () => {
        const reordered = [lineNames[1], lineNames[0], lineNames[2]]
        expect(lineSetMatches(lineNames, reordered)).toBe(false)
    })
})

// Source-level, in the style of chartTeardown.spec.ts: TimeChart cannot be mounted in a test
// (it needs the stores, i18n and a real canvas), and these two rules are what keep the fix
// connected. Per-file presence checks, so they catch the gross case rather than proving
// every call site.
const sourceFiles = import.meta.glob('../../**/*.{ts,vue}', {
    query: '?raw',
    import: 'default',
    eager: true,
}) as Record<string, string>

describe('chart line set wiring', () => {
    it('addresses chart data through the guarded lookup only', () => {
        const entry = Object.entries(sourceFiles).find(([path]) => path.endsWith('TimeChart.vue'))
        expect(entry, 'TimeChart.vue not found').toBeDefined()
        expect(entry![1]).not.toContain('uLineNames.indexOf(')
    })

    it('remounts the chart from every view that renders one', () => {
        const offenders = Object.entries(sourceFiles)
            .filter(
                ([path, source]) => !path.includes('__tests__') && source.includes('<TimeChart'),
            )
            .filter(([, source]) => !source.includes('line-set-changed'))
            .map(([path]) => path)
        expect(offenders, 'renders a TimeChart but never remounts it').toEqual([])
    })
})
