// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, ref, type EffectScope, type Ref } from 'vue'
import { unappliedProfileMode, useSustained } from '../cooling/powerProfiles.ts'

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

// The Modes page warning: a profile switch is a mismatch until its Mode has been activated, and
// that window must not be reported as a Mode that is not active.
describe('useSustained', () => {
    const GRACE_MS = 3000
    let scope: EffectScope

    const sustain = (source: Ref<string | undefined>) =>
        scope.run(() => useSustained(() => source.value, GRACE_MS))!

    beforeEach(() => {
        vi.useFakeTimers()
        scope = effectScope()
    })
    afterEach(() => {
        scope.stop()
        vi.useRealTimers()
    })

    it('never shows a mismatch shorter than the grace', () => {
        const source = ref<string | undefined>()
        const shown = sustain(source)

        source.value = 'Loud'
        vi.advanceTimersByTime(GRACE_MS - 1)
        expect(shown.value).toBeUndefined()
        source.value = undefined
        vi.advanceTimersByTime(GRACE_MS * 2)
        expect(shown.value).toBeUndefined()
    })

    it('shows a mismatch that lasts the grace', () => {
        const source = ref<string | undefined>()
        const shown = sustain(source)

        source.value = 'Loud'
        vi.advanceTimersByTime(GRACE_MS - 1)
        expect(shown.value).toBeUndefined()
        vi.advanceTimersByTime(1)
        expect(shown.value).toBe('Loud')
    })

    // The page can be opened in the middle of an activation, or right after a daemon start.
    it('waits out the grace for a mismatch already there at the start', () => {
        const shown = sustain(ref<string | undefined>('Loud'))

        expect(shown.value).toBeUndefined()
        vi.advanceTimersByTime(GRACE_MS)
        expect(shown.value).toBe('Loud')
    })

    it('hides at once when the mismatch clears, and waits again for the next one', () => {
        const source = ref<string | undefined>('Loud')
        const shown = sustain(source)
        vi.advanceTimersByTime(GRACE_MS)
        expect(shown.value).toBe('Loud')

        source.value = undefined
        expect(shown.value).toBeUndefined()

        source.value = 'Loud'
        expect(shown.value).toBeUndefined()
        vi.advanceTimersByTime(GRACE_MS - 1)
        expect(shown.value).toBeUndefined()
        vi.advanceTimersByTime(1)
        expect(shown.value).toBe('Loud')
    })

    it('measures the grace from the latest mismatch, not from an earlier one', () => {
        const source = ref<string | undefined>('Loud')
        const shown = sustain(source)
        vi.advanceTimersByTime(GRACE_MS - 1)

        source.value = undefined
        source.value = 'Loud'
        vi.advanceTimersByTime(1)
        expect(shown.value).toBeUndefined()
    })

    it('keeps showing when the mismatch moves to another Mode without clearing', () => {
        const source = ref<string | undefined>('Loud')
        const shown = sustain(source)
        vi.advanceTimersByTime(GRACE_MS)

        source.value = 'Quiet'
        expect(shown.value).toBe('Quiet')
    })

    it('leaves no timer pending once its scope has ended', () => {
        sustain(ref<string | undefined>('Loud'))
        expect(vi.getTimerCount()).toBe(1)

        scope.stop()
        expect(vi.getTimerCount()).toBe(0)
    })
})
