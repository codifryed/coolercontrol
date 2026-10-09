// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import 'reflect-metadata'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { defineComponent, h } from 'vue'
import { createI18n } from 'vue-i18n'
import { PowerProfileStateDTO } from '@/models/PowerProfile.ts'
import DaemonClient from '../DaemonClient.ts'
import { useSettingsStore } from '../SettingsStore.ts'

// The router pulls in every page of the app, which the store does not need here.
vi.mock('@/router', () => ({ default: {} }))

const state = (
    available: Array<string>,
    active: string | null,
    modes: Record<string, string>,
): PowerProfileStateDTO => Object.assign(new PowerProfileStateDTO(), { available, active, modes })

describe('power profile refresh', () => {
    const cleanups: Array<() => void> = []

    /** Runs in a component setup, as the pages do: the store needs one. */
    const settingsStore = (): ReturnType<typeof useSettingsStore> => {
        let store!: ReturnType<typeof useSettingsStore>
        const Host = defineComponent({
            setup() {
                store = useSettingsStore()
                return () => h('div')
            },
        })
        const i18n = createI18n({ legacy: false, locale: 'en', missingWarn: false })
        const host = mount(Host, { global: { plugins: [createPinia(), i18n] } })
        cleanups.push(() => host.unmount())
        return store
    }

    afterEach(() => {
        for (const cleanup of cleanups.splice(0)) cleanup()
        vi.restoreAllMocks()
    })

    /// Goal: a visit to the Modes page must not put back an active profile or a mapping that
    /// an event or a save made newer while the request was under way.
    it('replaces only the offered profiles', async () => {
        const getPowerProfiles = vi.spyOn(DaemonClient.prototype, 'getPowerProfiles')
        const store = settingsStore()
        getPowerProfiles.mockResolvedValueOnce(
            state(['power-saver', 'balanced'], 'balanced', { balanced: 'mode-1' }),
        )
        await store.loadPowerProfiles()
        store.applySystemEvent({ kind: 'power_profile', value: 'performance', previous: null })

        getPowerProfiles.mockResolvedValueOnce(state(['balanced', 'performance'], 'balanced', {}))
        await store.refreshAvailablePowerProfiles()

        expect(store.powerProfilesAvailable).toEqual(['balanced', 'performance'])
        expect(store.powerProfileActive).toBe('performance')
        expect(store.powerProfileModes).toEqual({ balanced: 'mode-1' })
    })

    /// Goal: a failed request is not a system without profiles, so the list on screen stays.
    it('keeps the offered profiles on a failed fetch', async () => {
        const getPowerProfiles = vi.spyOn(DaemonClient.prototype, 'getPowerProfiles')
        const store = settingsStore()
        getPowerProfiles.mockResolvedValueOnce(state(['power-saver', 'balanced'], 'balanced', {}))
        await store.loadPowerProfiles()

        getPowerProfiles.mockResolvedValueOnce(undefined)
        await store.refreshAvailablePowerProfiles()

        expect(store.powerProfilesAvailable).toEqual(['power-saver', 'balanced'])
    })
})
