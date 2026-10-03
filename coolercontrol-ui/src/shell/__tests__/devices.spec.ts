// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Device.ts uses class-transformer decorators which need the metadata polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import { type Device, DeviceType } from '@/models/Device.ts'
import {
    customSensorNames,
    deviceChannelLinks,
    deviceSensorLinks,
    hardwareDevices,
    sensorToggles,
} from '../devices/devices.ts'

interface FakeChannel {
    speed_options?: object
    lighting_modes?: object[]
    lcd_modes?: object[]
}

function fakeDevice(
    uid: string,
    type: DeviceType,
    temps: string[],
    channels: Record<string, FakeChannel>,
    statusChannels: string[] = [],
): Device {
    return {
        uid,
        type,
        info: {
            temps: new Map(temps.map((name) => [name, {}])),
            channels: new Map(
                Object.entries(channels).map(([name, channel]) => [
                    name,
                    { lighting_modes: [], lcd_modes: [], ...channel },
                ]),
            ),
        },
        status: {
            temps: temps.map((name) => ({ name })),
            channels: statusChannels.map((name) => ({ name })),
        },
    } as unknown as Device
}

describe('hardwareDevices', () => {
    it('includes custom-sensor devices', () => {
        const hwmon = fakeDevice('d1', DeviceType.HWMON, [], {})
        const custom = fakeDevice('c1', DeviceType.CUSTOM_SENSORS, ['sensor1'], {})
        expect(hardwareDevices([hwmon, custom]).map((d) => d.uid)).toEqual(['d1', 'c1'])
        expect(customSensorNames(custom)).toEqual(['sensor1'])
        expect(customSensorNames(hwmon)).toEqual([])
    })
})

describe('deviceChannelLinks', () => {
    it('lists lighting channels before lcd channels', () => {
        const device = fakeDevice('d1', DeviceType.LIQUIDCTL, [], {
            lcd: { lcd_modes: [{}] },
            led1: { lighting_modes: [{}] },
            fan1: { speed_options: {} },
        })
        expect(deviceChannelLinks(device)).toEqual([
            { deviceUID: 'd1', channelName: 'led1', kind: 'lighting' },
            { deviceUID: 'd1', channelName: 'lcd', kind: 'lcd' },
        ])
    })
})

describe('sensorToggles', () => {
    it('collects temps, keyword channels, fans, lighting, lcd without duplicates', () => {
        const device = fakeDevice(
            'd1',
            DeviceType.HWMON,
            ['temp1'],
            {
                fan1: { speed_options: {} },
                led1: { lighting_modes: [{}] },
            },
            ['CPU Load', 'GPU Freq'],
        )
        expect(sensorToggles(device, []).map((tog) => tog.channelName)).toEqual([
            'temp1',
            'GPU Freq',
            'CPU Load',
            'fan1',
            'led1',
        ])
        expect(sensorToggles(device, []).every((tog) => tog.enabled)).toBe(true)
    })

    it('appends persisted disabled channels sorted and unchecked', () => {
        const device = fakeDevice('d1', DeviceType.HWMON, ['temp1'], {})
        const toggles = sensorToggles(device, ['zeta', 'alpha'])
        expect(toggles.map((tog) => [tog.channelName, tog.enabled])).toEqual([
            ['temp1', true],
            ['alpha', false],
            ['zeta', false],
        ])
    })

    it('does not duplicate a disabled name that is also detected', () => {
        const device = fakeDevice('d1', DeviceType.HWMON, ['temp1'], {})
        expect(sensorToggles(device, ['temp1'])).toEqual([{ channelName: 'temp1', enabled: true }])
    })
})

describe('deviceSensorLinks', () => {
    const device = fakeDevice('d1', DeviceType.HWMON, ['temp1'], {
        fan1: { speed_options: { fixed_enabled: true } },
        fan2: { speed_options: { fixed_enabled: false } },
        led1: { lighting_modes: [{}] },
    })
    const kinds = (unitOf: (uid: string, channelName: string) => string | undefined) =>
        deviceSensorLinks(device, unitOf).map((link) => [link.channelName, link.kind])

    it('sends fan channels to Cooling', () => {
        expect(kinds(() => undefined)).toEqual([
            ['temp1', 'monitoring'],
            ['fan1', 'cooling'],
            ['fan2', 'cooling'],
            ['led1', 'lighting'],
        ])
    })

    it('sends an uncontrollable channel in another unit to Monitoring', () => {
        expect(kinds(() => 'dL/h')).toEqual([
            ['temp1', 'monitoring'],
            ['fan1', 'cooling'],
            ['fan2', 'monitoring'],
            ['led1', 'lighting'],
        ])
    })

    it('lists an info-only channel in another unit under Monitoring', () => {
        const liquid = fakeDevice('d2', DeviceType.LIQUIDCTL, [], {
            flow: {},
            plain: {},
            led1: { lighting_modes: [{}] },
        })
        const links = (unitOf: (uid: string, channelName: string) => string | undefined) =>
            deviceSensorLinks(liquid, unitOf).map((link) => [link.channelName, link.kind])
        // Every label names a unit here: only the channel with a reading becomes a sensor.
        expect(
            links((_uid, channelName) => (channelName === 'plain' ? undefined : 'dL/h')),
        ).toEqual([
            ['flow', 'monitoring'],
            ['led1', 'lighting'],
        ])
        expect(links(() => undefined)).toEqual([['led1', 'lighting']])
    })
})
