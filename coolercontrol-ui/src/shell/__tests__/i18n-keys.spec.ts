// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Every static translation key used by shell code must resolve in the
// english locale. Template-literal (dynamic) keys are not covered.

// devices.ts pulls in Device.ts, whose class-transformer decorators need the polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import en from '@/i18n/locales/en.ts'
import { TOUR_KEY_PREFIX, TOUR_STEPS } from '@/shell/tour.ts'
import { SENSOR_LINK_KINDS } from '@/shell/devices/devices.ts'
import { CHAIN_PILL_KINDS } from '@/shell/cooling/channels.ts'
import { POWER_PROFILE_NAMES } from '@/shell/cooling/powerProfiles.ts'
import { OVERDRIVE_I18N_KEYS } from '@/models/AmdGpuOverdrive.ts'

const shellFiles = import.meta.glob('../**/*.{ts,vue}', {
    query: '?raw',
    import: 'default',
    eager: true,
}) as Record<string, string>

const localeFiles = import.meta.glob('../../i18n/locales/*.ts', { eager: true }) as Record<
    string,
    { default: any }
>

const KEY_PATTERN = /\bt\(\s*'([a-zA-Z0-9._-]+)'/g
const LOCALE_COUNT = 12

function resolves(root: any, key: string): boolean {
    let node: any = root
    for (const part of key.split('.')) {
        node = node?.[part]
        if (node == null) return false
    }
    return typeof node === 'string'
}

function flatten(node: any, prefix = ''): string[] {
    return Object.entries(node).flatMap(([key, value]) =>
        value != null && typeof value === 'object'
            ? flatten(value, `${prefix}${key}.`)
            : [`${prefix}${key}`],
    )
}

describe('shell i18n keys', () => {
    it('resolves every static key against the english locale', () => {
        const missing: string[] = []
        let found = 0
        for (const [path, source] of Object.entries(shellFiles)) {
            if (path.includes('__tests__')) continue
            for (const match of source.matchAll(KEY_PATTERN)) {
                const key = match[1]
                found += 1
                if (!resolves(en, key)) missing.push(`${path}: ${key}`)
            }
        }
        expect(found).toBeGreaterThan(20)
        expect(missing).toEqual([])
    })

    // The tour builds its keys from its step list at runtime, so the static
    // sweep above cannot see them and an unused-key cleanup cannot either.
    // Checking every locale is the point: a prune once took these out of all
    // twelve.
    it('resolves every tour step key in every locale', () => {
        const locales = Object.entries(localeFiles).filter(([path]) => !path.endsWith('.d.ts'))
        expect(locales).toHaveLength(LOCALE_COUNT)

        const missing: string[] = []
        for (const [path, module] of locales) {
            for (const step of TOUR_STEPS) {
                for (const suffix of ['', 'Desc']) {
                    const key = `${TOUR_KEY_PREFIX}${step.key}${suffix}`
                    if (!resolves(module.default, key)) missing.push(`${path}: ${key}`)
                }
            }
        }
        expect(missing).toEqual([])
    })

    // The two checks below are the general form of the hand-listed ones above: any key english
    // has and a locale does not falls back to english mid-screen, and any key a locale still has
    // after english dropped it is dead weight a prune missed. A locale added 37 keys behind
    // english before this existed, all of them invisible because nothing compared the sets.
    it('has every english key in every locale', () => {
        const locales = Object.entries(localeFiles).filter(([path]) => !path.endsWith('.d.ts'))
        expect(locales).toHaveLength(LOCALE_COUNT)

        const englishKeys = new Set(flatten(en))
        expect(englishKeys.size).toBeGreaterThan(500)
        const missing: string[] = []
        for (const [path, module] of locales) {
            const keys = new Set(flatten(module.default))
            for (const key of englishKeys) {
                if (!keys.has(key)) missing.push(`${path}: ${key}`)
            }
        }
        expect(missing).toEqual([])
    })

    it('has no key in a locale that english dropped', () => {
        const locales = Object.entries(localeFiles).filter(([path]) => !path.endsWith('.d.ts'))
        const englishKeys = new Set(flatten(en))
        const stale: string[] = []
        for (const [path, module] of locales) {
            for (const key of flatten(module.default)) {
                if (!englishKeys.has(key)) stale.push(`${path}: ${key}`)
            }
        }
        expect(stale).toEqual([])
    })

    // ModesPage builds the profile label key as a template literal from whatever the daemon
    // reports, so the static sweep cannot see these three. A locale missing one shows the
    // english word beside translated ones, which reads as a bug rather than a gap.
    it('resolves every power profile name key in every locale', () => {
        const locales = Object.entries(localeFiles).filter(([path]) => !path.endsWith('.d.ts'))
        expect(locales).toHaveLength(LOCALE_COUNT)
        expect(POWER_PROFILE_NAMES.length).toBeGreaterThan(0)

        const missing: string[] = []
        for (const [path, module] of locales) {
            for (const profile of POWER_PROFILE_NAMES) {
                const key = `layout.shell.coolingPage.powerProfiles.profileNames.${profile}`
                if (!resolves(module.default, key)) missing.push(`${path}: ${key}`)
            }
        }
        expect(missing).toEqual([])
    })

    // The enum labels are picked in a model helper rather than a shell file, so
    // the sweep above never sees them either. A locale missing one falls back to
    // english mid-dropdown, which reads as a bug rather than a missing string.
    it('resolves the interface font keys in every locale', () => {
        const locales = Object.entries(localeFiles).filter(([path]) => !path.endsWith('.d.ts'))
        expect(locales).toHaveLength(LOCALE_COUNT)

        const keys = [
            'layout.settings.interfaceFont',
            'layout.settings.tooltips.interfaceFont',
            'models.interfaceFont.bundled',
            'models.interfaceFont.system',
        ]
        const missing: string[] = []
        for (const [path, module] of locales) {
            for (const key of keys) {
                if (!resolves(module.default, key)) missing.push(`${path}: ${key}`)
            }
        }
        expect(missing).toEqual([])
    })

    // The overdrive model builds its keys at runtime, so the static sweep cannot see them. The
    // list is held to what the helpers build by the model's own spec.
    it('resolves every overdrive key in every locale', () => {
        const locales = Object.entries(localeFiles).filter(([path]) => !path.endsWith('.d.ts'))
        expect(locales).toHaveLength(LOCALE_COUNT)
        expect(OVERDRIVE_I18N_KEYS.length).toBeGreaterThan(0)

        const missing: string[] = []
        for (const [path, module] of locales) {
            for (const key of OVERDRIVE_I18N_KEYS) {
                if (!resolves(module.default, key)) missing.push(`${path}: ${key}`)
            }
        }
        expect(missing).toEqual([])
    })

    // ChainStrip and DevicePage build these keys as template literals, so the static sweep
    // above cannot see them and neither can an unused-key prune. Both sets are driven from
    // the same exported arrays the components use, so the check cannot drift from the code.
    it('resolves every dynamic chain and sensor destination key in every locale', () => {
        const locales = Object.entries(localeFiles).filter(([path]) => !path.endsWith('.d.ts'))
        expect(locales).toHaveLength(LOCALE_COUNT)

        const keys = [
            ...CHAIN_PILL_KINDS.map((kind) => `layout.shell.coolingPage.chain.${kind}`),
            ...SENSOR_LINK_KINDS.map((kind) => `layout.shell.sensorDest.${kind}`),
        ]
        expect(keys.length).toBe(CHAIN_PILL_KINDS.length + SENSOR_LINK_KINDS.length)

        const missing: string[] = []
        for (const [path, module] of locales) {
            for (const key of keys) {
                if (!resolves(module.default, key)) missing.push(`${path}: ${key}`)
            }
        }
        expect(missing).toEqual([])
    })
})
