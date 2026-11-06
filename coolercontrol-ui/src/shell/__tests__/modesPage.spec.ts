// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, shallowMount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import en from '@/i18n/locales/en.ts'
import { tooltipDirective } from '@/shell/tooltipDirective.ts'
import { mdiInformationOutline } from '@mdi/js'

const loadPowerProfiles = vi.fn(() => Promise.resolve())
const refreshAvailablePowerProfiles = vi.fn(() => Promise.resolve())

vi.mock('@/stores/SettingsStore.ts', () => ({
    useSettingsStore: () => ({
        modes: [],
        modeActiveCurrent: undefined,
        modeActivePrevious: undefined,
        powerProfilesAvailable: ['power-saver', 'balanced'],
        powerProfileActive: 'balanced',
        powerProfileModes: {},
        ccSettings: { apply_on_boot: true },
        loadPowerProfiles,
        refreshAvailablePowerProfiles,
    }),
}))

vi.mock('@/composables/useToolWizards.ts', () => ({
    useToolWizards: () => ({ openModeWizard: vi.fn() }),
}))

vi.mock('@/shell/toast', () => ({ useToast: () => ({ add: vi.fn() }) }))

const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } })

const mountPage = async () => {
    const { default: ModesPage } = await import('@/shell/cooling/ModesPage.vue')
    return shallowMount(ModesPage, {
        global: { plugins: [i18n], stubs: { RouterLink: true } },
    })
}

// Full mount for the power profile card: shallowMount stubs drop named slots,
// and the info icon lives in a named slot. The tooltip directive is the real
// one, so the test reads the value it receives, and the icon renders as a
// path-bearing stub. Heavy children are stubbed individually.
const mountCard = async () => {
    const { default: ModesPage } = await import('@/shell/cooling/ModesPage.vue')
    return mount(ModesPage, {
        global: {
            plugins: [i18n],
            directives: { tooltip: tooltipDirective },
            stubs: {
                RouterLink: true,
                UiSelect: true,
                UiButton: true,
                'svg-icon': { props: ['path'], template: '<i :data-path="path" />' },
            },
        },
    })
}

beforeEach(() => {
    loadPowerProfiles.mockClear()
    refreshAvailablePowerProfiles.mockClear()
})

describe('ModesPage power profiles', () => {
    // The daemon pushes only the active profile. A system whose power profile daemon came back
    // offering other profiles reaches the page by fetching them again on a visit.
    it('fetches the offered profiles on every visit', async () => {
        const firstVisit = await mountPage()
        expect(refreshAvailablePowerProfiles).toHaveBeenCalledTimes(1)

        firstVisit.unmount()
        await mountPage()
        expect(refreshAvailablePowerProfiles).toHaveBeenCalledTimes(2)
    })

    // The full load also assigns the active profile and the mapping, which an event or a save
    // can have made newer than the response.
    it('does not reload the active profile or the mapping on a visit', async () => {
        await mountPage()
        expect(loadPowerProfiles).not.toHaveBeenCalled()
    })

    // The cooldown is daemon behavior with no setting to configure, so the page itself has to
    // carry the explanation: an info icon on the card's first row whose tooltip is the
    // translated cooldown text.
    it('documents the cooldown on an info icon', async () => {
        const wrapper = await mountCard()

        const label = wrapper.find('.cursor-help')
        expect(label.exists()).toBe(true)
        const tooltip = (label.element as HTMLElement & { $ccTooltip?: { value: string } })
            .$ccTooltip
        expect(tooltip?.value).toBe(en.layout.shell.coolingPage.powerProfiles.cooldownTooltip)
        expect(tooltip?.position).toBe('top')

        const icon = wrapper.find(`i[data-path="${mdiInformationOutline}"]`)
        expect(icon.exists()).toBe(true)
    })
})
