// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Device.ts uses class-transformer decorators which need the metadata polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import { type Device, DeviceType } from '@/models/Device.ts'
import { monitoringSensors } from '../monitoring/sensors.ts'

interface FakeChannel {
    speed_options?: object
    lighting_modes?: object[]
    lcd_info?: object
}

function fakeDevice(
    uid: string,
    type: DeviceType,
    temps: string[],
    channels: Record<string, FakeChannel>,
): Device {
    return {
        uid,
        type,
        info: {
            temps: new Map(temps.map((name) => [name, {}])),
            channels: new Map(
                Object.entries(channels).map(([name, channel]) => [
                    name,
                    { lighting_modes: [], ...channel },
                ]),
            ),
        },
    } as unknown as Device
}

describe('monitoringSensors', () => {
    it('includes temps, fans and value channels, excludes lighting/lcd', () => {
        const device = fakeDevice('d1', DeviceType.HWMON, ['temp1'], {
            fan1: { speed_options: { fixed_enabled: true } },
            'CPU Load': {},
            freq1: {},
            led1: { lighting_modes: [{}] },
            lcd1: { lcd_info: {} },
        })
        const groups = monitoringSensors([device])
        expect(groups).toHaveLength(1)
        expect(groups[0].sensors.map((s) => s.channelName)).toEqual([
            'temp1',
            'fan1',
            'CPU Load',
            'freq1',
        ])
        expect(groups[0].sensors[0].isTemp).toBe(true)
        expect(groups[0].sensors[1].isTemp).toBe(false)
    })

    it('excludes devices without sensors', () => {
        const lightingOnly = fakeDevice('d1', DeviceType.HWMON, [], {
            led1: { lighting_modes: [{}] },
        })
        const noInfo = { uid: 'd2', type: DeviceType.HWMON, info: null } as unknown as Device
        expect(monitoringSensors([lightingOnly, noInfo])).toEqual([])
    })

    it('includes custom-sensor devices like any other device', () => {
        const custom = fakeDevice('c1', DeviceType.CUSTOM_SENSORS, ['sensor1', 'sensor2'], {})
        const groups = monitoringSensors([custom])
        expect(groups).toHaveLength(1)
        expect(groups[0].sensors.map((s) => s.channelName)).toEqual(['sensor1', 'sensor2'])
        expect(groups[0].sensors.every((s) => s.isTemp)).toBe(true)
    })

    it('keeps the custom-sensor device without sensors, so new ones can be added', () => {
        const custom = fakeDevice('c1', DeviceType.CUSTOM_SENSORS, [], {})
        expect(monitoringSensors([custom])).toEqual([{ deviceUID: 'c1', sensors: [] }])
    })
})
