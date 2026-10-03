// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A plugin's frame is untrusted: it can post an openLink message at any time, with anything
// in it. These tests pin what the UI does with one before the user is asked.

import { afterEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import en from '@/i18n/locales/en.ts'
import { closeDialog, openDialogs } from '@/shell/dialog'
import { activeToasts, useToast } from '@/shell/toast'
import { usePluginLinks } from '@/composables/usePluginLinks.ts'

// The real store drags the daemon client and the router in for one address.
vi.mock('@/stores/DeviceStore.ts', () => ({
    useDeviceStore: () => ({ daemonClient: { daemonURL: 'http://nas.lan:11987/' } }),
}))

const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } })

// The composable reads a store, so it only runs inside a component setup.
function pluginLinks(): ReturnType<typeof usePluginLinks> {
    let captured!: ReturnType<typeof usePluginLinks>
    mount(
        defineComponent({
            setup() {
                captured = usePluginLinks()
                return () => h('div')
            },
        }),
        { global: { plugins: [i18n] } },
    )
    return captured
}

function setUserActivation(isActive: boolean | undefined): void {
    Object.defineProperty(navigator, 'userActivation', {
        value: isActive == null ? undefined : { isActive },
        configurable: true,
    })
}

afterEach(() => {
    // Closing is what lets the next request through, as it does in the app.
    for (const dialog of [...openDialogs]) closeDialog(dialog.id)
    useToast().removeAll()
    setUserActivation(undefined)
})

describe('usePluginLinks', () => {
    it('asks about a valid link, with the parsed URL and the plugin that sent it', () => {
        setUserActivation(true)
        pluginLinks().requestLink('my-plugin', 'https://EXAMPLE.com/docs')

        expect(openDialogs).toHaveLength(1)
        const data = openDialogs[0].options.data
        expect(data.pluginId).toBe('my-plugin')
        expect(data.url.href).toBe('https://example.com/docs')
        expect(activeToasts).toHaveLength(0)
    })

    it('asks about one link at a time, and again once that is closed', () => {
        setUserActivation(true)
        const links = pluginLinks()
        links.requestLink('my-plugin', 'https://example.com/one')
        links.requestLink('my-plugin', 'https://example.com/two')
        // Another plugin surface shares the limit.
        pluginLinks().requestLink('other-plugin', 'https://example.com/three')
        expect(openDialogs).toHaveLength(1)
        expect(openDialogs[0].options.data.url.pathname).toBe('/one')

        closeDialog(openDialogs[0].id)
        links.requestLink('my-plugin', 'https://example.com/two')
        expect(openDialogs).toHaveLength(1)
        expect(openDialogs[0].options.data.url.pathname).toBe('/two')
    })

    it.each([
        ['the UI host', 'http://localhost:9999/steal'],
        ['the daemon host', 'http://nas.lan:9999/steal'],
        ['a relative path', '/plugins/my-plugin/ui/index.html'],
        ['a local scheme', 'file:///etc/passwd'],
        ['no string', { href: 'https://example.com/' }],
    ])('blocks a link to %s and says so, without asking', (_, raw) => {
        setUserActivation(true)
        pluginLinks().requestLink('my-plugin', raw)

        expect(openDialogs).toHaveLength(0)
        expect(activeToasts).toHaveLength(1)
        expect(activeToasts[0].severity).toBe('warn')
        expect(activeToasts[0].summary).toBe('Link blocked')
    })

    it('ignores a request that no click preceded', () => {
        setUserActivation(false)
        const links = pluginLinks()
        links.requestLink('my-plugin', 'https://example.com/')
        links.requestLink('my-plugin', 'file:///etc/passwd')

        expect(openDialogs).toHaveLength(0)
        expect(activeToasts).toHaveLength(0)
    })

    it('still asks in a browser that cannot report user activation', () => {
        setUserActivation(undefined)
        pluginLinks().requestLink('my-plugin', 'https://example.com/')

        expect(openDialogs).toHaveLength(1)
    })
})
