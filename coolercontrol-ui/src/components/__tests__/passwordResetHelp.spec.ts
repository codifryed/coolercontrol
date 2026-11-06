// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Container installs (Docker, Bazzite's Podman quadlet) have no host coolercontrold binary, so
// the forgot-password help must offer a command for each. These tests pin that every command is
// shown and that each copy button copies its own command.

import { afterEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import en from '@/i18n/locales/en.ts'
import PasswordDialog from '@/components/PasswordDialog.vue'

vi.mock('@/stores/DeviceStore.ts', () => ({
    useDeviceStore: () => ({ reloadUI: vi.fn() }),
}))

const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } })

const COMMANDS = [
    'sudo coolercontrold --reset-password',
    'sudo docker exec coolercontrold coolercontrold --reset-password',
    'sudo podman exec coolercontrold coolercontrold --reset-password',
]

async function mountWithHelpOpen(): Promise<VueWrapper> {
    const dialogRef = ref({ data: { setPasswd: false }, close: vi.fn() })
    const wrapper = mount(PasswordDialog, {
        global: {
            plugins: [i18n],
            provide: { dialogRef },
            directives: { tooltip: {} },
        },
    })
    const toggle = wrapper.findAll('span[role="button"]').find((span) => {
        return span.text() === en.components.password.forgotPassword
    })
    expect(toggle).toBeDefined()
    await toggle!.trigger('click')
    return wrapper
}

afterEach(() => {
    vi.unstubAllGlobals()
})

describe('PasswordDialog reset help', () => {
    it('shows the package, Docker and Podman commands', async () => {
        const wrapper = await mountWithHelpOpen()

        const shown = wrapper.findAll('code').map((code) => code.text())

        expect(shown).toEqual(COMMANDS)
        expect(wrapper.text()).toContain(en.components.password.forgotPasswordPackageLabel)
        expect(wrapper.text()).toContain('Docker')
        expect(wrapper.text()).toContain('Podman (Bazzite)')
        expect(wrapper.text()).toContain(en.components.password.forgotPasswordContainerNote)
        wrapper.unmount()
    })

    it('copies the command next to the clicked button', async () => {
        const writeText = vi.fn().mockResolvedValue(undefined)
        vi.stubGlobal('navigator', { clipboard: { writeText } })
        const wrapper = await mountWithHelpOpen()
        const copyButtons = wrapper.findAll(
            `button[aria-label="${en.components.password.forgotPasswordCopyCommand}"]`,
        )
        expect(copyButtons).toHaveLength(COMMANDS.length)

        for (const button of copyButtons) {
            await button.trigger('click')
        }
        await flushPromises()

        expect(writeText.mock.calls.map((call) => call[0])).toEqual(COMMANDS)
        wrapper.unmount()
    })
})
