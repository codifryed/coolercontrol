// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The per-thread frequency toggle saves and restarts the daemon. A refused save must put the
// stored setting back, or the next save from any surface would persist it silently.

import 'reflect-metadata'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import en from '@/i18n/locales/en.ts'
import { CoolerControlDeviceSettingsDTO } from '@/models/CCSettings.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import { useDeviceActions } from '@/composables/useDeviceActions.ts'

const UID = 'cpu-uid'
const state = vi.hoisted(() => ({
    accepts: true,
    saveResult: true as unknown,
    saved: [] as Array<boolean>,
    shutdowns: 0,
    settings: new Map<string, unknown>(),
}))

vi.mock('@/shell/confirm', () => ({
    useConfirm: () => ({
        require: (options: { accept: () => unknown; reject?: () => void }) =>
            state.accepts ? options.accept() : options.reject?.(),
    }),
}))
vi.mock('@/shell/toast', () => ({ useToast: () => ({ add: () => {} }) }))
vi.mock('@/stores/DeviceStore.ts', () => ({
    useDeviceStore: () => ({
        daemonClient: {
            saveCCDeviceSettings: async (
                _uid: string,
                setting: CoolerControlDeviceSettingsDTO,
            ): Promise<unknown> => {
                state.saved.push(setting.extensions.per_thread_freq)
                return state.saveResult
            },
            shutdownDaemon: async (): Promise<void> => {
                state.shutdowns++
            },
        },
        sleep: async (): Promise<void> => {},
        waitAndReload: async (): Promise<void> => {},
    }),
}))
vi.mock('@/stores/SettingsStore.ts', () => ({
    useSettingsStore: () => ({ ccDeviceSettings: state.settings }),
}))

const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } })

// The composable reads stores, so it only runs inside a component setup.
function deviceActions(): ReturnType<typeof useDeviceActions> {
    let captured!: ReturnType<typeof useDeviceActions>
    mount(
        defineComponent({
            setup() {
                captured = useDeviceActions()
                return () => h('div')
            },
        }),
        { global: { plugins: [i18n] } },
    )
    return captured
}

function storedSetting(): CoolerControlDeviceSettingsDTO {
    return state.settings.get(UID) as CoolerControlDeviceSettingsDTO
}

// The confirm callback is async; let it finish.
const settle = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0))

beforeEach(() => {
    state.accepts = true
    state.saveResult = true
    state.saved = []
    state.shutdowns = 0
    state.settings = new Map([[UID, new CoolerControlDeviceSettingsDTO(UID, 'CPU')]])
})

describe('setPerThreadFreq', () => {
    it('saves the new value and restarts the daemon', async () => {
        const onReject = vi.fn()

        deviceActions().setPerThreadFreq(UID, true, onReject)
        await settle()

        expect(state.saved).toEqual([true])
        expect(storedSetting().extensions.per_thread_freq).toBe(true)
        expect(state.shutdowns).toBe(1)
        expect(onReject).not.toHaveBeenCalled()
    })

    it('puts the stored value back when the daemon refuses the save', async () => {
        state.saveResult = new ErrorResponse('refused')
        const onReject = vi.fn()

        deviceActions().setPerThreadFreq(UID, true, onReject)
        await settle()

        expect(state.saved).toEqual([true])
        expect(storedSetting().extensions.per_thread_freq).toBe(false)
        expect(state.shutdowns).toBe(0)
        expect(onReject).toHaveBeenCalledOnce()
    })

    it('saves nothing when the restart is declined', async () => {
        state.accepts = false
        const onReject = vi.fn()

        deviceActions().setPerThreadFreq(UID, true, onReject)
        await settle()

        expect(state.saved).toEqual([])
        expect(storedSetting().extensions.per_thread_freq).toBe(false)
        expect(onReject).toHaveBeenCalledOnce()
    })
})
