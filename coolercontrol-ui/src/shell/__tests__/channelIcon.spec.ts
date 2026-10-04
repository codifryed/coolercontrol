// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import type { Device } from '@/models/Device.ts'
import { channelKind, channelKindIcon, channelSpins } from '@/shell/channelIcon.ts'

const device = {
    uid: 'd1',
    info: {
        temps: new Map([['temp1', {}]]),
        channels: new Map([
            ['fan1', { speed_options: { fixed_enabled: true } }],
            ['fan2', { speed_options: { fixed_enabled: false } }],
            ['flow', {}],
            ['power1', {}],
            ['freq1', {}],
            ['load1', {}],
        ]),
    },
} as unknown as Device
const noUnit = (): undefined => undefined
// fan2 and the info-only flow channel are labelled in dL/h.
const flowUnit = (_uid: string, channelName: string): string | undefined =>
    channelName === 'fan2' || channelName === 'flow' ? 'dL/h' : undefined

describe('channelKind', () => {
    it('reads the kind from device metadata and the reported field', () => {
        expect(channelKind(device, 'temp1', { temp: '40.0' }, noUnit)).toBe('temp')
        expect(channelKind(device, 'fan1', { duty: '50', rpm: '900' }, noUnit)).toBe('fan')
        expect(channelKind(device, 'fan2', { rpm: '900' }, noUnit)).toBe('fan')
        expect(channelKind(device, 'power1', { watts: '12.0' }, noUnit)).toBe('power')
        expect(channelKind(device, 'freq1', { freq: '4200' }, noUnit)).toBe('freq')
        expect(channelKind(device, 'load1', { duty: '7' }, noUnit)).toBe('load')
    })

    it('calls a speed value that is not a fan a sensor', () => {
        expect(channelKind(device, 'fan2', { rpm: '120' }, flowUnit)).toBe('sensor')
        expect(channelKind(device, 'flow', { rpm: '120' }, flowUnit)).toBe('sensor')
        expect(channelKindIcon('sensor')).not.toBe(channelKindIcon('fan'))
    })

    it('keeps a fan a fan before its first reading', () => {
        expect(channelKind(device, 'fan1', undefined, noUnit)).toBe('fan')
    })
})

describe('channelSpins', () => {
    it('never spins a sensor', () => {
        expect(channelSpins('fan', { rpm: '120' }, true)).toBe(true)
        expect(channelSpins('sensor', { rpm: '120' }, true)).toBe(false)
    })
})
