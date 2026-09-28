// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import {
    attributeLabel,
    formatAttributeNumber,
    formatAttributeValue,
    thresholdLinesFrom,
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
const nct6687Fan: Array<ChannelAttribute> = [
    { name: 'fan1_min', kind: 'FAN_MIN', value: 402 },
    { name: 'fan1_max', kind: 'FAN_MAX', value: 1505 },
]
const amdgpuFan: Array<ChannelAttribute> = [
    { name: 'fan1_min', kind: 'FAN_MIN', value: 0 },
    { name: 'fan1_max', kind: 'FAN_MAX', value: 3500 },
    { name: 'fan1_target', kind: 'FAN_TARGET', value: 939 },
]

describe('thresholdLinesFrom', () => {
    it('draws upper temperature limits on the percent scale, never the lower ones', () => {
        const lines = thresholdLinesFrom(nvme, 1, t)
        expect(lines.map((line) => line.name)).toEqual(['temp1_max', 'temp1_crit'])
        expect(lines[0]).toMatchObject({ scale: '%', severity: 'warning', value: 89.85 })
        expect(lines[1]).toMatchObject({ severity: 'critical', label: 'temp1_crit 94.85 °C' })
    })

    it('draws fan limits on the rpm scale in chart units and skips a zero limit', () => {
        const nct = thresholdLinesFrom(nct6687Fan, 1000, t)
        expect(nct.map((line) => line.value)).toEqual([0.402, 1.505])
        expect(nct[0]).toMatchObject({ scale: 'rpm', severity: 'fan' })
        // fan1_min=0 sits on the axis; the target is only listed, not drawn.
        expect(thresholdLinesFrom(amdgpuFan, 1, t).map((line) => line.name)).toEqual(['fan1_max'])
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
})
