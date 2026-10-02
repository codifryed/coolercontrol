// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import {
    attributeLabel,
    formatAttributeNumber,
    formatAttributeValue,
    limitColor,
    limitLinesFrom,
} from '@/components/channelAttributes.ts'
import type { ChannelAttribute } from '@/models/ChannelAttributes.ts'
import en from '@/i18n/locales/en.ts'

// Resolves keys against the english locale, so a renamed key fails here too.
const t = (key: string): string =>
    key.split('.').reduce((node: any, part) => node?.[part], en) ?? `MISSING:${key}`

// As read from real hardware.
const nvme: Array<ChannelAttribute> = [
    { name: 'temp1_max', kind: 'TEMP_MAX', value: 89.85 },
    { name: 'temp1_crit', kind: 'TEMP_CRIT', value: 94.85 },
    { name: 'temp1_min', kind: 'TEMP_MIN', value: -5.15 },
]
const nct6687Temp: Array<ChannelAttribute> = [
    { name: 'temp1_max', kind: 'TEMP_MAX', value: 95 },
    { name: 'temp1_min', kind: 'TEMP_MIN', value: 39 },
]
const nct6687Fan: Array<ChannelAttribute> = [
    { name: 'fan1_min', kind: 'FAN_MIN', value: 402 },
    { name: 'fan1_max', kind: 'FAN_MAX', value: 1505 },
]
const amdgpuFan: Array<ChannelAttribute> = [
    { name: 'fan1_min', kind: 'FAN_MIN', value: 0 },
    { name: 'fan1_max', kind: 'FAN_MAX', value: 3500 },
    { name: 'fan1_target', kind: 'FAN_TARGET', value: 939 },
]
const amdgpuPower: Array<ChannelAttribute> = [
    { name: 'power1_cap', kind: 'POWER_CAP', value: 230 },
    { name: 'power1_cap_max', kind: 'POWER_CAP_MAX', value: 230 },
    { name: 'power1_cap_min', kind: 'POWER_CAP_MIN', value: 216 },
]

describe('limitLinesFrom', () => {
    it('draws temperature limits on the percent scale and skips a minimum below the axis', () => {
        const lines = limitLinesFrom(nvme, 1, t)
        expect(lines.map((line) => line.name)).toEqual(['temp1_max', 'temp1_crit'])
        expect(lines[0]).toMatchObject({ scale: '%', severity: 'warning', value: 89.85 })
        expect(lines[1]).toMatchObject({ severity: 'critical', label: 'temp1_crit 94.85 °C' })
    })

    it('draws a temperature minimum like a fan minimum', () => {
        const lines = limitLinesFrom(nct6687Temp, 1000, t)
        expect(lines.map((line) => line.name)).toEqual(['temp1_max', 'temp1_min'])
        expect(lines[1]).toMatchObject({
            scale: '%',
            severity: 'neutral',
            value: 39,
            label: 'temp1_min 39.0 °C',
        })
        const zero: Array<ChannelAttribute> = [{ name: 'temp5_min', kind: 'TEMP_MIN', value: 0 }]
        expect(limitLinesFrom(zero, 1, t)).toEqual([])
    })

    it('draws fan limits on the rpm scale in chart units and skips a zero limit', () => {
        const nct = limitLinesFrom(nct6687Fan, 1000, t)
        expect(nct.map((line) => line.value)).toEqual([0.402, 1.505])
        expect(nct[0]).toMatchObject({ scale: 'rpm', severity: 'neutral' })
        // fan1_min=0 sits on the axis; the target is only listed, not drawn.
        expect(limitLinesFrom(amdgpuFan, 1, t).map((line) => line.name)).toEqual(['fan1_max'])
    })

    it('only lists power and rated attributes', () => {
        const rated: Array<ChannelAttribute> = [
            { name: 'temp1_rated_max', kind: 'TEMP_RATED_MAX', value: 125 },
        ]
        expect(limitLinesFrom(amdgpuPower, 1, t)).toEqual([])
        expect(limitLinesFrom(rated, 1, t)).toEqual([])
    })
})

describe('limitColor', () => {
    it('gives lines and panel swatches one colour per severity', () => {
        const palette = { red: 'rgb(1 0 0)', yellow: 'rgb(0 1 0)', text_color_secondary: 'grey' }
        expect(limitColor('critical', palette)).toBe('rgb(1 0 0)')
        expect(limitColor('warning', palette)).toBe('rgb(0 1 0)')
        expect(limitColor('neutral', palette)).toBe('grey')
    })
})

describe('attribute formatting', () => {
    it('keeps a reported second decimal on temperatures and none on counts', () => {
        expect(formatAttributeNumber(nvme[1])).toBe('94.85')
        expect(formatAttributeNumber(nvme[2])).toBe('-5.15')
        expect(formatAttributeNumber({ name: 'temp2_crit', kind: 'TEMP_CRIT', value: 110 })).toBe(
            '110.0',
        )
        expect(formatAttributeNumber(nct6687Fan[1])).toBe('1505')
    })

    it('labels every kind and spells out known sensor types', () => {
        expect(attributeLabel('TEMP_CRIT', t)).toBe('Critical')
        expect(attributeLabel('FAN_PULSES', t)).not.toContain('MISSING')
        expect(formatAttributeValue({ name: 'temp1_type', kind: 'TEMP_TYPE', value: 3 }, t)).toBe(
            'Thermal diode',
        )
        expect(formatAttributeValue({ name: 'temp1_type', kind: 'TEMP_TYPE', value: 9 }, t)).toBe(
            '9',
        )
        expect(formatAttributeValue({ name: 'fan1_pulses', kind: 'FAN_PULSES', value: 2 }, t)).toBe(
            '2',
        )
        expect(formatAttributeValue(amdgpuFan[2], t)).toBe('939 rpm')
    })

    it('shows power in watts and rated temperatures in degrees', () => {
        expect(formatAttributeValue(amdgpuPower[2], t)).toBe('216.0 W')
        expect(
            formatAttributeValue(
                { name: 'power1_cap_hyst', kind: 'POWER_CAP_HYST', value: 0.5 },
                t,
            ),
        ).toBe('0.5 W')
        expect(
            formatAttributeValue(
                { name: 'temp1_rated_min', kind: 'TEMP_RATED_MIN', value: -40 },
                t,
            ),
        ).toBe('-40.0 °C')
        expect(attributeLabel('POWER_CAP', t)).toBe('Power Cap')
        expect(attributeLabel('TEMP_RATED_MAX', t)).toBe('Rated Max')
    })
})
