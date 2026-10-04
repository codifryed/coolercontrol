// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { newCustomSensorId } from '@/components/customSensorEditor.ts'

const hashes = (...values: string[]): (() => string) => {
    let next = 0
    return () => values[Math.min(next++, values.length - 1)]
}

describe('newCustomSensorId', () => {
    it('prefixes an eight character hash', () => {
        expect(newCustomSensorId(new Set())).toMatch(/^sensor_[0-9a-f]{8}$/)
    })

    it('does not follow the numbered ids of existing sensors', () => {
        const taken = new Set(['sensor1', 'sensor2', 'sensor10', 'Auto Delta temp1 temp2'])
        expect(newCustomSensorId(taken, hashes('3f9a1c2e'))).toBe('sensor_3f9a1c2e')
    })

    it('draws again when the id is taken', () => {
        const taken = new Set(['sensor_aaaaaaaa'])
        expect(newCustomSensorId(taken, hashes('aaaaaaaa', 'bbbbbbbb'))).toBe('sensor_bbbbbbbb')
    })

    it('gives up when every draw is taken', () => {
        const taken = new Set(['sensor_aaaaaaaa'])
        expect(() => newCustomSensorId(taken, hashes('aaaaaaaa'))).toThrow()
    })
})
