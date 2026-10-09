// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import en from '@/i18n/locales/en.ts'

const loadPowerProfiles = vi.fn(() => Promise.resolve())

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

beforeEach(() => {
    loadPowerProfiles.mockClear()
})

describe('ModesPage power profiles', () => {
    // The daemon pushes only the active profile. A system whose power profile daemon came back
    // offering other profiles reaches the page by fetching them again on a visit.
    it('fetches the offered profiles on every visit', async () => {
        const firstVisit = await mountPage()
        expect(loadPowerProfiles).toHaveBeenCalledTimes(1)

        firstVisit.unmount()
        await mountPage()
        expect(loadPowerProfiles).toHaveBeenCalledTimes(2)
    })
})
