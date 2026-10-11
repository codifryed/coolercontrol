// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Dashboard.ts carries class-transformer decorators, which need the polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import { ChartType, knownChartType } from '@/models/Dashboard.ts'

describe('knownChartType', () => {
    it('keeps the chart types this version draws', () => {
        expect(knownChartType('Time Chart')).toBe(ChartType.TIME_CHART)
        expect(knownChartType('Table')).toBe(ChartType.TABLE)
    })

    it('falls back to Time Chart for removed or unknown types', () => {
        expect(knownChartType('Controls')).toBe(ChartType.TIME_CHART)
        expect(knownChartType('Scatter')).toBe(ChartType.TIME_CHART)
        expect(knownChartType('')).toBe(ChartType.TIME_CHART)
    })
})
