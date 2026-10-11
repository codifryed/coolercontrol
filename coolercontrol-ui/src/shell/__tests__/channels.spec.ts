// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import type { Device } from '@/models/Device.ts'
import { coolingChannels, pinId } from '../cooling/channels.ts'

const noUnit = (): undefined => undefined

function fakeDevice(uid: string, channels: Record<string, object | undefined>): Device {
    return {
        uid,
        info: {
            channels: new Map(
                Object.entries(channels).map(([name, speedOptions]) => [
                    name,
                    { speed_options: speedOptions },
                ]),
            ),
        },
    } as unknown as Device
}

describe('coolingChannels', () => {
    it('includes only channels with speed options', () => {
        const device = fakeDevice('d1', {
            fan1: { fixed_enabled: true, min_duty: 0, max_duty: 100 },
            temp1: undefined,
        })
        const groups = coolingChannels([device], noUnit)
        expect(groups).toHaveLength(1)
        expect(groups[0].channels.map((c) => c.channelName)).toEqual(['fan1'])
        expect(groups[0].channels[0].controllable).toBe(true)
    })

    it('marks channels without fixed_enabled as not controllable', () => {
        const device = fakeDevice('d1', {
            fan1: { fixed_enabled: false, min_duty: 20, max_duty: 80 },
        })
        const channel = coolingChannels([device], noUnit)[0].channels[0]
        expect(channel.controllable).toBe(false)
        expect(channel.minDuty).toBe(20)
        expect(channel.maxDuty).toBe(80)
    })

    it('leaves out an uncontrollable channel whose label names another unit', () => {
        const device = fakeDevice('d1', {
            fan1: { fixed_enabled: false, min_duty: 0, max_duty: 100 },
            fan2: { fixed_enabled: false, min_duty: 0, max_duty: 100 },
            fan3: { fixed_enabled: true, min_duty: 0, max_duty: 100 },
        })
        // fan2 is a flow meter. fan3 is driven, so its unit does not make it a sensor.
        const unitOf = (_uid: string, channelName: string): string | undefined =>
            channelName === 'fan1' ? undefined : 'dL/h'
        const names = coolingChannels([device], unitOf)[0].channels.map((c) => c.channelName)
        expect(names).toEqual(['fan1', 'fan3'])
    })

    it('skips devices without fan channels or info', () => {
        const noFans = fakeDevice('d1', { temp1: undefined })
        const noInfo = { uid: 'd2', info: null } as unknown as Device
        expect(coolingChannels([noFans, noInfo], noUnit)).toEqual([])
    })

    it('builds pin ids in the legacy tree format', () => {
        expect(pinId('abc', 'fan1')).toBe('abc_fan1')
    })
})
