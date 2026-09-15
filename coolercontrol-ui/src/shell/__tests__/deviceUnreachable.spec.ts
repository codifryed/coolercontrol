// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A device that stops answering and a channel serving failsafe values are different conditions,
// and the UI has to keep them apart. Failsafe means the device is alive with stale readings and
// safe values substituted; unreachable means nothing can be read from or written to it. Showing
// the second as the first would tell a user their fans are on a safe curve when in fact no curve
// can be applied.
//
// The daemon reports both at once for a wedged device, because its channels also failsafe once
// their readings go stale. These tests pin the precedence that resolves that overlap.

import 'reflect-metadata'
import { plainToInstance } from 'class-transformer'
import { describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import en from '@/i18n/locales/en.ts'
import {
    DeviceHealthDTO,
    HealthState,
    UnreachableDelta,
    UnreachableRef,
} from '@/models/DeviceHealth.ts'
import { useDeviceHealth } from '@/composables/useDeviceHealth.ts'

const healthUnreachable: Array<{ device_uid: string }> = []
const healthFailsafe: Array<{ device_uid: string; name: string; reason?: string }> = []

// The real store drags the router and the whole shell in for two arrays.
vi.mock('@/stores/SettingsStore.ts', () => ({
    useSettingsStore: () => ({ healthUnreachable, healthFailsafe }),
}))

const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } })

// The composable reads a store, so it only runs inside a component setup.
function callInSetup<T>(use: (api: ReturnType<typeof useDeviceHealth>) => T): T {
    let captured!: T
    mount(
        defineComponent({
            setup() {
                captured = use(useDeviceHealth())
                return () => h('div')
            },
        }),
        { global: { plugins: [i18n] } },
    )
    return captured
}

describe('device unreachable health state', () => {
    it('has an english string for both the label and its explanation', () => {
        const appInfo = (en as any).views.appInfo
        expect(appInfo.deviceUnreachable).toBeTruthy()
        expect(appInfo.deviceUnreachableDetail).toBeTruthy()
        // The explanation has to say why a setting has no effect, not merely that something is off.
        expect(appInfo.deviceUnreachableDetail).not.toBe(appInfo.deviceUnreachable)
    })

    it('parses the daemon snapshot into the model, including the timeout count', () => {
        const dto = plainToInstance(DeviceHealthDTO, {
            failsafe: [],
            unreachable: [{ device_uid: 'dev1', device_name: 'octo', consecutive_timeouts: 8 }],
            missing: [],
            stale_source: [],
            firmware_overrides: [],
            channel_capabilities: [],
            system_findings: [],
        } as object)

        expect(dto.unreachable).toHaveLength(1)
        expect(dto.unreachable[0]).toBeInstanceOf(UnreachableRef)
        expect(dto.unreachable[0].device_name).toBe('octo')
        expect(dto.unreachable[0].consecutive_timeouts).toBe(8)
    })

    // An older daemon sends no `unreachable` field at all. The UI must read that as "nothing
    // unreachable" rather than crashing on an undefined array.
    it('defaults to empty when the daemon does not report the field', () => {
        const dto = plainToInstance(DeviceHealthDTO, { failsafe: [] } as object)
        expect(dto.unreachable).toEqual([])
    })

    // Every surface that shows this resolves it through `useDeviceHealth`, so the rule is
    // asserted against that one implementation rather than a copy of it.
    it('reports the device state ahead of its channels failsafe, because it is the cause', () => {
        healthUnreachable.length = 0
        healthFailsafe.length = 0
        healthUnreachable.push({ device_uid: 'dev1' })
        healthFailsafe.push(
            { device_uid: 'dev1', name: 'fan1', reason: 'stale' },
            { device_uid: 'dev2', name: 'fan1', reason: 'stale' },
        )

        const tips = callInSetup((api) => ({
            wedged: api.healthTooltip('dev1', 'fan1'),
            healthy: api.healthTooltip('dev2', 'fan1'),
            unreachableText: api.unreachableText(),
        }))

        // The wedged device says "not responding", not "failsafe values in use".
        expect(tips.wedged).toBe(tips.unreachableText)
        // A healthy device's failsafe is still its own story.
        expect(tips.healthy).toContain(en.views.appInfo.failsafeActive)
        expect(tips.healthy).not.toBe(tips.unreachableText)
    })

    it('treats a wedged device as unhealthy even before any channel failsafes', () => {
        healthUnreachable.length = 0
        healthFailsafe.length = 0
        healthUnreachable.push({ device_uid: 'dev1' })

        const flags = callInSetup((api) => ({
            wedged: api.isDeviceUnhealthy('dev1'),
            other: api.isDeviceUnhealthy('dev2'),
        }))

        expect(flags.wedged).toBe(true)
        expect(flags.other).toBe(false)
    })

    it('carries the resolved state so a recovered device clears rather than latching', () => {
        const delta = plainToInstance(UnreachableDelta, {
            device_uid: 'dev1',
            device_name: 'octo',
            consecutive_timeouts: 8,
            state: 'Resolved',
        } as object)
        expect(delta.state).toBe(HealthState.Resolved)
        expect(delta.device_uid).toBe('dev1')
    })
})
