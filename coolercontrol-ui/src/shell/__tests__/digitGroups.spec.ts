// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { DIGIT_GROUP_SEPARATOR as S, groupDigits, ungroupDigits } from '@/shell/digitGroups.ts'

describe('groupDigits', () => {
    it('groups five digits and more in threes', () => {
        expect(groupDigits(10000)).toBe(`10${S}000`)
        expect(groupDigits(438300)).toBe(`438${S}300`)
        expect(groupDigits(9999999)).toBe(`9${S}999${S}999`)
        expect(groupDigits('6553500')).toBe(`6${S}553${S}500`)
    })

    it('leaves up to four digits alone', () => {
        expect(groupDigits(0)).toBe('0')
        expect(groupDigits(1500)).toBe('1500')
        expect(groupDigits('9999')).toBe('9999')
    })

    it('groups only the integer part', () => {
        expect(groupDigits('12345.678')).toBe(`12${S}345.678`)
        expect(groupDigits('65.4')).toBe('65.4')
        expect(groupDigits(-12345)).toBe(`-12${S}345`)
    })

    it('returns anything that is not a plain number unchanged', () => {
        expect(groupDigits('')).toBe('')
        expect(groupDigits('n/a')).toBe('n/a')
        expect(groupDigits('1e21')).toBe('1e21')
    })
})

describe('ungroupDigits', () => {
    it('reverses groupDigits', () => {
        for (const value of [0, 1500, 10000, 438300, 9999999]) {
            expect(Number(ungroupDigits(groupDigits(value)))).toBe(value)
        }
    })

    it('strips ordinary and no-break spaces from a pasted number', () => {
        expect(ungroupDigits('438 300')).toBe('438300')
        expect(ungroupDigits('438\u00A0300 ')).toBe('438300')
    })
})
