// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A plugin's frame is untrusted: it can post an openLink message at any time, with anything
// in it. These tests pin what the UI does with one before the user is asked.

import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import en from '@/i18n/locales/en.ts'
import { closeDialog, openDialogs } from '@/shell/dialog'
import { activeToasts, useToast } from '@/shell/toast'
import {
    frameClickOf,
    PLUGIN_LINK_COOLDOWN_MS,
    PLUGIN_LINK_LOCK_MS,
    usePluginLinks,
} from '@/composables/usePluginLinks.ts'

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

// A plugin's message as each kind of browser delivers it.
const NO_FIELD = undefined
const FRAME_CLICK = true
const NO_FRAME_CLICK = false

function hostInput(type: 'pointerdown' | 'keydown' = 'pointerdown'): void {
    window.dispatchEvent(new Event(type))
}

// One clock for the whole file: the limits are module state and outlive a test.
beforeAll(() => {
    vi.useFakeTimers()
})
afterAll(() => {
    vi.useRealTimers()
})

afterEach(() => {
    // Closing, then every limit running out, is what lets the next request through.
    for (const dialog of [...openDialogs]) closeDialog(dialog.id)
    vi.advanceTimersByTime(PLUGIN_LINK_LOCK_MS)
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

    it('asks about one link at a time, and again after the cooldown', () => {
        setUserActivation(true)
        const links = pluginLinks()
        links.requestLink('my-plugin', 'https://example.com/one')
        links.requestLink('my-plugin', 'https://example.com/two')
        // Another plugin surface shares the limit.
        pluginLinks().requestLink('other-plugin', 'https://example.com/three')
        expect(openDialogs).toHaveLength(1)
        expect(openDialogs[0].options.data.url.pathname).toBe('/one')

        closeDialog(openDialogs[0].id)
        vi.advanceTimersByTime(PLUGIN_LINK_COOLDOWN_MS)
        links.requestLink('my-plugin', 'https://example.com/two')
        expect(openDialogs).toHaveLength(1)
        expect(openDialogs[0].options.data.url.pathname).toBe('/two')
    })

    it('asks at once after a cancel when a click in the frame came first', () => {
        setUserActivation(true)
        const links = pluginLinks()
        for (const path of ['/one', '/two', '/three']) {
            links.requestLink('my-plugin', `https://example.com${path}`, false, FRAME_CLICK)
            expect(openDialogs).toHaveLength(1)
            expect(openDialogs[0].options.data.url.pathname).toBe(path)
            closeDialog(openDialogs[0].id)
        }
    })

    it('ignores a plugin whose frame was not clicked, whatever the UI was', () => {
        // A click on the UI leaves the window activated, not the frame.
        setUserActivation(true)
        const links = pluginLinks()
        links.requestLink('my-plugin', 'https://example.com/', false, NO_FRAME_CLICK)
        links.requestLink('my-plugin', 'file:///etc/passwd', false, NO_FRAME_CLICK)
        hostInput()
        links.requestLink('my-plugin', 'https://example.com/', false, NO_FRAME_CLICK)

        expect(openDialogs).toHaveLength(0)
        expect(activeToasts).toHaveLength(0)
    })

    it('asks once more in the cooldown, then not until the lock runs out', () => {
        // The click that closed the prompt leaves the window activated.
        setUserActivation(true)
        const links = pluginLinks()
        links.requestLink('my-plugin', 'https://example.com/one', false, NO_FIELD)
        closeDialog(openDialogs[0].id)

        // The second try at a link.
        links.requestLink('my-plugin', 'https://example.com/two', false, NO_FIELD)
        expect(openDialogs).toHaveLength(1)
        expect(openDialogs[0].options.data.url.pathname).toBe('/two')
        closeDialog(openDialogs[0].id)

        links.requestLink('my-plugin', 'https://example.com/three', false, NO_FIELD)
        pluginLinks().requestLink('other-plugin', 'https://example.com/four', false, NO_FIELD)
        vi.advanceTimersByTime(PLUGIN_LINK_LOCK_MS - 1)
        links.requestLink('my-plugin', 'https://example.com/three', false, NO_FIELD)
        expect(openDialogs).toHaveLength(0)

        vi.advanceTimersByTime(1)
        links.requestLink('my-plugin', 'https://example.com/three', false, NO_FIELD)
        expect(openDialogs).toHaveLength(1)
    })

    it.each(['pointerdown', 'keydown'] as const)(
        'asks once more after a %s on the UI lifts the lock',
        (type) => {
            setUserActivation(true)
            const links = pluginLinks()
            links.requestLink('my-plugin', 'https://example.com/one', false, NO_FIELD)
            closeDialog(openDialogs[0].id)
            links.requestLink('my-plugin', 'https://example.com/two', false, NO_FIELD)
            closeDialog(openDialogs[0].id)
            links.requestLink('my-plugin', 'https://example.com/three', false, NO_FIELD)
            expect(openDialogs).toHaveLength(0)

            hostInput(type)
            links.requestLink('my-plugin', 'https://example.com/three', false, NO_FIELD)
            expect(openDialogs).toHaveLength(1)
            closeDialog(openDialogs[0].id)

            // One prompt for that input, not a new run of them.
            links.requestLink('my-plugin', 'https://example.com/four', false, NO_FIELD)
            expect(openDialogs).toHaveLength(0)
        },
    )

    it('keeps the lock through the input that closes the prompt', () => {
        setUserActivation(true)
        const links = pluginLinks()
        links.requestLink('my-plugin', 'https://example.com/one', false, NO_FIELD)
        closeDialog(openDialogs[0].id)
        links.requestLink('my-plugin', 'https://example.com/two', false, NO_FIELD)

        // Cancel, or Escape: either starts while the prompt is open.
        hostInput('pointerdown')
        hostInput('keydown')
        closeDialog(openDialogs[0].id)

        links.requestLink('my-plugin', 'https://example.com/three', false, NO_FIELD)
        expect(openDialogs).toHaveLength(0)
    })

    it('asks about a link the UI itself offers during the cooldown, one at a time', () => {
        setUserActivation(true)
        const links = pluginLinks()
        links.requestLink('my-plugin', 'https://example.com/one')
        closeDialog(openDialogs[0].id)

        links.requestLink('my-plugin', 'https://example.com/home', true)
        links.requestLink('my-plugin', 'https://example.com/again', true)
        expect(openDialogs).toHaveLength(1)
        expect(openDialogs[0].options.data.url.pathname).toBe('/home')
    })

    it('still blocks a link the UI itself offers when it is not one to open', () => {
        setUserActivation(true)
        pluginLinks().requestLink('my-plugin', 'http://nas.lan:9999/steal', true)

        expect(openDialogs).toHaveLength(0)
        expect(activeToasts).toHaveLength(1)
    })

    it.each([
        ['with a click in the frame', FRAME_CLICK],
        ['in a browser that cannot say', NO_FIELD],
    ])('says a link was blocked once per cooldown, %s', (_, isFrameClick) => {
        setUserActivation(true)
        const links = pluginLinks()
        for (let i = 0; i < 50; i++) {
            links.requestLink('my-plugin', 'file:///etc/passwd', false, isFrameClick)
        }
        expect(activeToasts).toHaveLength(1)

        // A valid link still gets its prompt in that time.
        links.requestLink('my-plugin', 'https://example.com/', false, isFrameClick)
        expect(openDialogs).toHaveLength(1)
        closeDialog(openDialogs[0].id)

        useToast().removeAll()
        vi.advanceTimersByTime(PLUGIN_LINK_COOLDOWN_MS - 1)
        links.requestLink('my-plugin', 'file:///etc/passwd', false, isFrameClick)
        expect(activeToasts).toHaveLength(0)

        vi.advanceTimersByTime(1)
        links.requestLink('my-plugin', 'file:///etc/passwd', false, isFrameClick)
        expect(activeToasts).toHaveLength(1)
    })

    it('says a link was blocked while locked', () => {
        setUserActivation(true)
        const links = pluginLinks()
        links.requestLink('my-plugin', 'https://example.com/one', false, NO_FIELD)
        closeDialog(openDialogs[0].id)
        links.requestLink('my-plugin', 'https://example.com/two', false, NO_FIELD)
        closeDialog(openDialogs[0].id)

        links.requestLink('my-plugin', 'file:///etc/passwd', false, NO_FIELD)
        expect(activeToasts).toHaveLength(1)
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

describe('frameClickOf', () => {
    const message = (fields: object) =>
        Object.assign(new MessageEvent('message', { data: { type: 'openLink' } }), fields)

    it('cannot say in a browser without the field', () => {
        const event = new MessageEvent('message')
        expect('userActivation' in event).toBe(false)
        expect(frameClickOf(event)).toBeUndefined()
    })

    it.each([
        ['a message sent without it', { userActivation: null }, false],
        ['a frame that was not clicked', { userActivation: { isActive: false } }, false],
        ['a frame that was clicked', { userActivation: { isActive: true } }, true],
    ])('reads %s', (_, fields, expected) => {
        expect(frameClickOf(message(fields))).toBe(expected)
    })
})
