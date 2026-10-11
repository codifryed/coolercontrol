// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useThemeColorsStore } from '../ThemeColorsStore.ts'

describe('rgbToHex', () => {
    beforeEach(() => setActivePinia(createPinia()))

    it('converts an rgb string to hex', () => {
        expect(useThemeColorsStore().rgbToHex('rgb(86, 138, 242)')).toBe('#568af2')
    })

    it('passes hex colors through unchanged', () => {
        expect(useThemeColorsStore().rgbToHex('#568af2')).toBe('#568af2')
    })

    // A sensor listed by the Monitoring panel but missing a color settings entry
    // yields an empty string. This must not throw: previously the null match was
    // dereferenced before the guard and blanked every Dashboard.
    it('returns an empty string unchanged instead of throwing', () => {
        expect(() => useThemeColorsStore().rgbToHex('')).not.toThrow()
        expect(useThemeColorsStore().rgbToHex('')).toBe('')
    })

    it('returns unparseable input unchanged instead of throwing', () => {
        expect(() => useThemeColorsStore().rgbToHex('not-a-color')).not.toThrow()
        expect(useThemeColorsStore().rgbToHex('not-a-color')).toBe('not-a-color')
    })
})

describe('themeColors', () => {
    const root = document.documentElement

    beforeEach(() => setActivePinia(createPinia()))
    afterEach(() => {
        root.removeAttribute('style')
        root.removeAttribute('class')
    })

    /// Goal: a chart built after a theme switch draws in that theme's colors.
    /// Method: the store is created under one theme, as it is at startup, before
    /// the saved theme is applied. A switch sets the new values and then changes
    /// the class on the root, which is all that announces it.
    it('follows a theme switch', async () => {
        root.style.setProperty('--colors-text-color', '220 225 236')
        root.style.setProperty('--colors-success', '50 200 100')
        const store = useThemeColorsStore()
        expect(store.themeColors.text_color).toBe('rgb(220 225 236)')
        expect(store.themeColors.green).toBe('rgb(50 200 100)')

        root.style.setProperty('--colors-text-color', '73 80 87')
        root.style.setProperty('--colors-success', '20 120 60')
        root.classList.add('light-theme')
        await new Promise((resolve) => setTimeout(resolve))

        expect(store.themeColors.text_color).toBe('rgb(73 80 87)')
        expect(store.themeColors.green).toBe('rgb(20 120 60)')
    })
})
