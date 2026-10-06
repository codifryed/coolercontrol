// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import 'reflect-metadata'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { defineComponent, h } from 'vue'
import { createI18n } from 'vue-i18n'
import { useSettingsStore } from '../SettingsStore.ts'

// The router pulls in every page of the app, which the store does not need here.
vi.mock('@/router', () => ({ default: {} }))

const PALETTE = {
    variant: 'dark',
    tokens: {
        accent: '#b9dafc',
        accentGradientTo: '#b9dafc',
        bgOne: '#292929',
        bgTwo: '#151515',
        borderOne: '#4d4d4d',
        textColor: '#ffffff',
        textColorSecondary: '#c7c7c7',
        success: '#87bb62',
        warning: '#ffcc17',
        error: '#f89b78',
        info: '#b6a6e9',
    },
}

describe('embedder palette', () => {
    const root = document.documentElement
    const embedder = { postMessage: vi.fn() } as unknown as Window
    const cleanups: Array<() => void> = []

    /** Runs in a component setup, as App.vue does: the store needs one. */
    const followEmbedderPalette = (): void => {
        const Host = defineComponent({
            setup() {
                useSettingsStore().followEmbedderPalette()
                return () => h('div')
            },
        })
        const i18n = createI18n({ legacy: false, locale: 'en', missingWarn: false })
        const host = mount(Host, { global: { plugins: [createPinia(), i18n] } })
        cleanups.push(() => host.unmount())
    }

    beforeEach(() => {
        vi.spyOn(window, 'parent', 'get').mockReturnValue(embedder)
        // The listener lives as long as the page does, so take it down by hand.
        const addEventListener = window.addEventListener.bind(window)
        vi.spyOn(window, 'addEventListener').mockImplementation((type, listener, options) => {
            addEventListener(type, listener, options)
            cleanups.push(() => window.removeEventListener(type, listener, options))
        })
    })

    afterEach(() => {
        for (const cleanup of cleanups.splice(0)) cleanup()
        vi.restoreAllMocks()
        root.removeAttribute('style')
        root.removeAttribute('class')
    })

    /// Goal: the palette an embedding page sends becomes the theme on screen,
    /// before any settings load, so the login dialog wears it too.
    it('applies the palette the embedding page sends', () => {
        followEmbedderPalette()
        expect(root.classList.contains('installed-theme')).toBe(false)

        window.dispatchEvent(
            new MessageEvent('message', {
                data: { type: 'coolercontrol:palette', palette: PALETTE },
                source: embedder,
                origin: window.location.origin,
            }),
        )

        expect(root.classList.contains('installed-theme')).toBe(true)
        expect(root.style.getPropertyValue('--colors-accent')).toBe('185 218 252')
        expect(root.style.getPropertyValue('--colors-bg-one')).toBe('41 41 41')
        expect(root.style.getPropertyValue('--colors-text-color')).toBe('255 255 255')
    })
})
