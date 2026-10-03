// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { channelUnit, labelUnit } from '@/shell/channelUnit.ts'

describe('labelUnit', () => {
    it('reads a short trailing bracket as written', () => {
        expect(labelUnit('Pressure [ubar]')).toBe('ubar')
        expect(labelUnit('Flow speed [dL/h]')).toBe('dL/h')
        expect(labelUnit('Conductivity [nS/cm] ')).toBe('nS/cm')
        expect(labelUnit('Water quality [%]')).toBe('%')
        expect(labelUnit('Fan [RPM]')).toBe('RPM')
        expect(labelUnit('[ml]')).toBe('ml')
    })

    it('takes ten characters and no more', () => {
        expect(labelUnit('Fan [tenletters]')).toBe('tenletters')
        expect(labelUnit('Fan [toolongunit1]')).toBeUndefined()
        expect(labelUnit('Level [µl/100km²]')).toBe('µl/100km²')
    })

    it('ignores brackets that are not a trailing unit', () => {
        expect(labelUnit('CPU Fan')).toBeUndefined()
        expect(labelUnit('Fan [Rear Exhaust]')).toBeUndefined()
        expect(labelUnit('Fan []')).toBeUndefined()
        expect(labelUnit('Fan ]')).toBeUndefined()
        expect(labelUnit('Fan [x]]')).toBeUndefined()
        expect(labelUnit('[dL/h] Flow')).toBeUndefined()
        expect(labelUnit('')).toBeUndefined()
        expect(labelUnit(undefined)).toBeUndefined()
    })
})

describe('channelUnit', () => {
    it('reads the detected label when the user set none', () => {
        expect(channelUnit('Flow speed [dL/h]', undefined)).toBe('dL/h')
        expect(channelUnit('Pressure [ubar]', {})).toBe('ubar')
        expect(channelUnit('CPU Fan', undefined)).toBeUndefined()
    })

    it('falls back to the detected label under a user label without a unit', () => {
        const overrides = { label: 'Loop flow', channel_label: 'Flow speed [dL/h]' }
        expect(channelUnit('Loop flow', overrides)).toBe('dL/h')
    })

    it('prefers the unit of the user label', () => {
        const overrides = { label: 'Loop flow [L/h]', channel_label: 'Flow speed [dL/h]' }
        expect(channelUnit('Loop flow [L/h]', overrides)).toBe('L/h')
        expect(channelUnit('Loop flow [L/h]', { label: 'Loop flow [L/h]' })).toBe('L/h')
    })

    it('is undefined for rpm in any case', () => {
        expect(channelUnit('Fan [rpm]', undefined)).toBeUndefined()
        expect(channelUnit('Fan [RPM]', undefined)).toBeUndefined()
    })

    it('lets a user label reset a detected unit to rpm', () => {
        const overrides = { label: 'Pump [rpm]', channel_label: 'Flow speed [dL/h]' }
        expect(channelUnit('Pump [rpm]', overrides)).toBeUndefined()
    })

    it('ignores a stale hint once the override is gone', () => {
        expect(channelUnit('Pump speed', { channel_label: 'Flow speed [dL/h]' })).toBeUndefined()
    })
})
