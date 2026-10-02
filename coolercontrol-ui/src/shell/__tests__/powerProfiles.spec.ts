// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { unappliedProfileMode } from '../cooling/powerProfiles.ts'

const MODES = { performance: 'mode-loud', balanced: 'mode-quiet', 'power-saver': '' }

describe('unappliedProfileMode', () => {
    it('reports the mapped Mode when another one is active', () => {
        expect(unappliedProfileMode('performance', MODES, 'mode-quiet')).toBe('mode-loud')
    })

    // Changing a setting by hand clears the active Mode, which is the case the warning is for.
    it('reports the mapped Mode when no Mode is active', () => {
        expect(unappliedProfileMode('performance', MODES, undefined)).toBe('mode-loud')
    })

    it('reports nothing when the mapped Mode is the active one', () => {
        expect(unappliedProfileMode('performance', MODES, 'mode-loud')).toBeUndefined()
    })

    it('reports nothing for a profile with no Mode', () => {
        expect(unappliedProfileMode('quiet', MODES, 'mode-loud')).toBeUndefined()
        // The daemon drops a cleared mapping, but a blank one must not read as a Mode either.
        expect(unappliedProfileMode('power-saver', MODES, 'mode-loud')).toBeUndefined()
    })

    it('reports nothing before a profile has been observed', () => {
        expect(unappliedProfileMode(undefined, MODES, 'mode-quiet')).toBeUndefined()
    })
})
