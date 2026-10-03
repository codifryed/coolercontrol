// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The prompt can appear under a click the user aimed at the plugin's page. These tests pin
// that its link is not there to be hit until the prompt has been up for a moment.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { mount, type VueWrapper } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import en from '@/i18n/locales/en.ts'
import { OPENER_ARM_DELAY_MS } from '@/composables/pluginLinkOpener.ts'
import PluginLinkDialog from '@/shell/plugins/PluginLinkDialog.vue'

const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } })

function mountDialog(): VueWrapper {
    const dialogRef = ref({
        data: { pluginId: 'my-plugin', url: new URL('https://example.com/docs') },
        close: vi.fn(),
    })
    return mount(PluginLinkDialog, {
        global: { plugins: [i18n], provide: { dialogRef } },
    })
}

function openButton(wrapper: VueWrapper): HTMLButtonElement {
    return wrapper.find('button[aria-hidden="true"]').element as HTMLButtonElement
}

beforeEach(() => {
    vi.useFakeTimers()
})
afterEach(() => {
    vi.useRealTimers()
})

describe('PluginLinkDialog', () => {
    it('has no opener, and a disabled Open button, when it appears', async () => {
        const wrapper = mountDialog()
        await vi.advanceTimersByTimeAsync(OPENER_ARM_DELAY_MS - 1)

        expect(wrapper.find('iframe').exists()).toBe(false)
        expect(openButton(wrapper).disabled).toBe(true)
        wrapper.unmount()
    })

    it('arms the opener with the link once the delay is over', async () => {
        const wrapper = mountDialog()
        await vi.advanceTimersByTimeAsync(OPENER_ARM_DELAY_MS)

        const frame = wrapper.find('iframe')
        expect(frame.exists()).toBe(true)
        expect(frame.attributes('sandbox')).toBe('allow-popups allow-popups-to-escape-sandbox')
        expect(frame.attributes('srcdoc')).toContain('href="https://example.com/docs"')
        expect(openButton(wrapper).disabled).toBe(false)
        wrapper.unmount()
    })

    it('does not arm once it is closed', async () => {
        const wrapper = mountDialog()
        wrapper.unmount()

        expect(vi.getTimerCount()).toBe(0)
    })
})
