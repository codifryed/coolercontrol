// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The app loads reflect-metadata via an injected script, tests must import it
// themselves before touching a model that uses class-transformer decorators.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import en from '@/i18n/locales/en.ts'
import { ChannelVerdict, verdictDocsLink } from '@/models/DeviceHealth.ts'

// The verdict labels are chosen in a switch inside the notice and badge
// components rather than read from a table, so the static i18n sweep does see
// them. These tests guard the two things the sweep cannot: that the set of
// verdicts and the set of keys stay in step, and that the docs mapping does
// not quietly lose an anchor.

const VERDICT_KEYS: Record<ChannelVerdict, string | undefined> = {
    [ChannelVerdict.Controllable]: undefined,
    [ChannelVerdict.FirmwareOverride]: 'verdictFirmwareOverride',
    [ChannelVerdict.FamilyMayNeedOutOfTree]: 'verdictFamilyMayNeedOutOfTree',
    [ChannelVerdict.NotSupportedByDriver]: 'verdictNotSupportedByDriver',
    [ChannelVerdict.NoPwm]: 'verdictNoPwm',
    [ChannelVerdict.PwmReadOnly]: 'verdictPwmReadOnly',
    [ChannelVerdict.IgnoresDuty]: 'verdictIgnoresDuty',
    [ChannelVerdict.Unverifiable]: 'verdictUnverifiable',
}

describe('channel verdicts', () => {
    it('has an english string for every verdict that needs one', () => {
        const page = (en as any).layout.shell.coolingPage
        for (const [verdict, key] of Object.entries(VERDICT_KEYS)) {
            if (key == null) continue
            expect(typeof page[key], `missing string for ${verdict}`).toBe('string')
        }
    })

    it('sends every actionable verdict to a docs anchor', () => {
        // Controllable needs no help, and Unverifiable is a gap in evidence
        // rather than a hardware limitation, so neither gets a link.
        expect(verdictDocsLink(ChannelVerdict.Controllable)).toBeUndefined()
        expect(verdictDocsLink(ChannelVerdict.Unverifiable)).toBeUndefined()
        for (const verdict of [
            ChannelVerdict.FirmwareOverride,
            ChannelVerdict.FamilyMayNeedOutOfTree,
            ChannelVerdict.NotSupportedByDriver,
            ChannelVerdict.NoPwm,
            ChannelVerdict.PwmReadOnly,
            ChannelVerdict.IgnoresDuty,
        ]) {
            const link = verdictDocsLink(verdict)
            expect(link, `no docs page for ${verdict}`).toBeDefined()
        }
    })

    it('sends an Apple device to the Apple page instead', () => {
        // These fans have no pwm file, so the laptops anchor describes
        // hardware they are not. Every other device keeps the general anchor.
        const general = verdictDocsLink(ChannelVerdict.PwmReadOnly)
        const intel = verdictDocsLink(ChannelVerdict.PwmReadOnly, 'applesmc')
        const silicon = verdictDocsLink(ChannelVerdict.NotSupportedByDriver, 'macsmc_hwmon')
        expect(intel).toBe('https://docs.coolercontrol.org/devices/apple.html#intel-macs')
        expect(silicon).toBe('https://docs.coolercontrol.org/devices/apple.html#apple-silicon')
        expect(verdictDocsLink(ChannelVerdict.PwmReadOnly, 'nct6798')).toBe(general)
        // A name that happens to be an Object property must not match either.
        expect(verdictDocsLink(ChannelVerdict.PwmReadOnly, 'constructor')).toBe(general)
    })

    it('keeps the Apple page for a device the user renamed', () => {
        // The daemon sends the user's label as the device name, so only the
        // driver's name still says what the device is.
        const general = verdictDocsLink(ChannelVerdict.PwmReadOnly)
        const verdict = ChannelVerdict.PwmReadOnly
        const intel = 'https://docs.coolercontrol.org/devices/apple.html#intel-macs'
        const silicon = 'https://docs.coolercontrol.org/devices/apple.html#apple-silicon'
        expect(verdictDocsLink(verdict, 'applesmc', 'MacBook Fans')).toBe(intel)
        // The Apple Silicon driver's name has a hyphen, its hwmon name does not.
        expect(verdictDocsLink(verdict, 'macsmc-hwmon', 'Mac Studio')).toBe(silicon)
        // Without a known driver name the device name still decides.
        expect(verdictDocsLink(verdict, undefined, 'macsmc_hwmon')).toBe(silicon)
        expect(verdictDocsLink(verdict, 'platform', 'applesmc')).toBe(intel)
        expect(verdictDocsLink(verdict, undefined, 'MacBook Fans')).toBe(general)
        expect(verdictDocsLink(verdict, 'nct6775', 'nct6798')).toBe(general)
        expect(verdictDocsLink(ChannelVerdict.Controllable, 'applesmc', 'x')).toBeUndefined()
    })

    it('gives an Apple device no link where the verdict has none', () => {
        expect(verdictDocsLink(ChannelVerdict.Controllable, 'applesmc')).toBeUndefined()
        expect(verdictDocsLink(ChannelVerdict.Unverifiable, 'macsmc_hwmon')).toBeUndefined()
    })

    it('never names a kernel module in a verdict string', () => {
        // The app asserts observations; the docs assert remedies. A module name
        // baked into the binary goes stale and can contradict the maintainers.
        const page = (en as any).layout.shell.coolingPage
        const forbidden = ['nct6', 'it87', 'modprobe', 'lm-sensors', 'lm_sensors']
        for (const key of Object.values(VERDICT_KEYS)) {
            if (key == null) continue
            const text = String(page[key]).toLowerCase()
            for (const term of forbidden) {
                expect(text.includes(term), `${key} names "${term}"`).toBe(false)
            }
        }
    })
})
