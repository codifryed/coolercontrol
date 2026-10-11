// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { parseSystemPalette, THEME_TOKEN_KEYS } from '../themes.ts'

/** The KDE reading the Qt app produces, used verbatim as the fixture. */
const FULL = JSON.stringify({
    accent: '#3daee9',
    variant: 'dark',
    tokens: {
        accent: '#3daee9',
        accentGradientTo: '#3daee9',
        bgOne: '#141618',
        bgTwo: '#202326',
        borderOne: '#292c30',
        error: '#da4453',
        info: '#1d99f3',
        success: '#27ae60',
        textColor: '#fcfcfc',
        textColorSecondary: '#a1a9b1',
        warning: '#f67400',
    },
})

describe('parseSystemPalette', () => {
    // A refused token set is reported on the console, which is not under test.
    beforeEach(() => {
        vi.spyOn(console, 'warn').mockImplementation(() => {})
    })
    afterEach(() => {
        vi.restoreAllMocks()
    })

    /// Goal: a full desktop palette survives intact, since it drives every
    /// `--colors-*` variable the System theme sets.
    it('accepts a complete palette', () => {
        const palette = parseSystemPalette(FULL)
        expect(palette?.variant).toBe('dark')
        expect(palette?.accent).toBe('#3daee9')
        for (const key of THEME_TOKEN_KEYS) {
            expect(palette?.tokens?.[key], key).toMatch(/^#[0-9a-f]{6}$/)
        }
    })

    /// Goal: the accent-only reading a non-KDE desktop produces is still usable.
    /// Method: the shape the portal path emits, with no tokens at all.
    it('accepts an accent and variant with no tokens', () => {
        const palette = parseSystemPalette('{"accent":"#3daee9","variant":"dark"}')
        expect(palette).toEqual({ accent: '#3daee9', variant: 'dark' })
        expect(palette?.tokens).toBeUndefined()
    })

    /// Goal: a partial token set is refused outright rather than mixed with the
    /// compiled theme, which would draw half desktop colors and half ours.
    it('drops the token set when one token is missing', () => {
        const partial = JSON.parse(FULL)
        delete partial.tokens.warning
        const palette = parseSystemPalette(JSON.stringify(partial))
        expect(palette?.tokens).toBeUndefined()
        expect(palette?.accent).toBe('#3daee9')
    })

    /// Goal: nothing that is not a 6-digit hex reaches a CSS variable. This is the
    /// one palette the UI does not ship, so it cannot be checked at build time.
    it('drops the token set on a malformed color', () => {
        const bad = JSON.parse(FULL)
        bad.tokens.error = 'red'
        expect(parseSystemPalette(JSON.stringify(bad))?.tokens).toBeUndefined()
    })

    /// Goal: an unusable accent does not become `rgb(NaN NaN NaN)`.
    it('drops a malformed accent', () => {
        expect(parseSystemPalette('{"accent":"#fff","variant":"light"}')).toEqual({
            variant: 'light',
        })
    })

    /// Goal: an unknown variant is ignored rather than treated as light.
    it('ignores an unknown variant', () => {
        expect(parseSystemPalette('{"variant":"sepia","accent":"#3daee9"}')).toEqual({
            accent: '#3daee9',
        })
    })

    /// Goal: the empty string the Qt app sends when the desktop exposes nothing
    /// leaves the UI on its existing behavior instead of throwing.
    it('returns null for nothing usable', () => {
        expect(parseSystemPalette('')).toBeNull()
        expect(parseSystemPalette('not json')).toBeNull()
        expect(parseSystemPalette('null')).toBeNull()
        expect(parseSystemPalette('{}')).toBeNull()
        expect(parseSystemPalette('{"variant":"sepia"}')).toBeNull()
    })
})
