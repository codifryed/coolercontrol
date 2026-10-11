// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from 'vitest'
import type uPlot from 'uplot'
import { escapeHtml, safeColor } from '@/components/htmlEscaping.ts'
import { type DeviceLineProperties, tooltipPlugin } from '@/components/u-plot-plugins.ts'

// uPlot reads matchMedia at import, which jsdom lacks. The tooltip only needs its types.
vi.mock('uplot', () => ({ default: {} }))

// The chart tooltip is built as a markup string and assigned to innerHTML. Its line name
// is a sensor or channel label, which originates from hwmon, a liquidctl device, a service
// plugin, or the user's own name overrides. None of those are trusted to be markup-free,
// and the tooltip renders same-origin against a daemon that writes to hardware as root.
describe('chart tooltip escaping', () => {
    it('neutralises markup in a line name', () => {
        expect(escapeHtml('<img src=x onerror="alert(1)">')).toBe(
            '&lt;img src=x onerror=&quot;alert(1)&quot;&gt;',
        )
    })

    it('escapes ampersands first so an entity cannot be reconstructed', () => {
        // &lt;script&gt; must not decode back into a tag: escaping < before & would
        // leave the & of an injected entity untouched.
        expect(escapeHtml('&lt;script&gt;')).toBe('&amp;lt;script&amp;gt;')
    })

    it('leaves an ordinary label alone', () => {
        expect(escapeHtml('CPU Package')).toBe('CPU Package')
        expect(escapeHtml('Fan #1 (rear)')).toBe('Fan #1 (rear)')
    })

    it('treats a missing name as empty rather than printing undefined', () => {
        expect(escapeHtml(undefined)).toBe('')
    })

    it('passes through the colour forms the picker produces', () => {
        for (const color of [
            '#fff',
            '#00aaff',
            '#00aaffcc',
            'rgb(0, 170, 255)',
            'rgba(0,170,255,0.5)',
        ]) {
            expect(safeColor(color)).toBe(color)
        }
    })

    it('rejects a colour that could break out of the style attribute', () => {
        // The value lands inside style="fill:...", where escaping alone would not stop a
        // declaration from being closed early.
        expect(safeColor('red;" onload="alert(1)')).toBe('currentColor')
        expect(safeColor('url(javascript:alert(1))')).toBe('currentColor')
        expect(safeColor(undefined)).toBe('currentColor')
    })

    it('neutralises markup in a unit taken from a label', () => {
        // An rpm line's unit is the bracket text of a channel label, as untrusted as the name.
        const lineName = 'uid_flow_rpm'
        const lines = new Map<string, DeviceLineProperties>([
            [lineName, { color: '#fff', name: 'Flow', unit: '<b>x</b>' }],
        ])
        const plugin = tooltipPlugin(lines, (key: string) => key, 1)
        const over = document.createElement('div')
        const chart = {
            over,
            cursor: { idx: 0, top: 10, left: 10 },
            scales: { rpm: { min: 0, max: 100 } },
            series: [{}, { show: true, scale: 'rpm', label: lineName }],
            data: [[0], [50]],
            posToVal: () => 50,
            width: 400,
            height: 300,
        } as unknown as uPlot
        plugin.hooks.init[0](chart, {} as uPlot.Options, [] as unknown as uPlot.AlignedData)
        plugin.hooks.setCursor[0](chart)

        const tooltip = over.querySelector('.u-plot-tooltip')!
        expect(tooltip.textContent).toContain('50 <b>x</b>')
        expect(tooltip.querySelector('b')).toBeNull()
    })
})
