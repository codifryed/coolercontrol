// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import {
    formatLoad,
    formatRamLoad,
    isCpu,
    isDimm,
    isGpu,
    liveLoad,
    raisePeak,
    trackedPeak,
} from '@/components/stressLoad.ts'
import { type Device, DeviceType, type UID } from '@/models/Device'
import type { DeviceInfo, DriverInfo } from '@/models/DeviceInfo'
import type { ChannelValues } from '@/stores/DeviceStore.ts'

const device = (uid: UID, type: DeviceType, driver?: string): Device =>
    ({
        uid,
        type,
        info: { driver_info: { name: driver } as DriverInfo } as DeviceInfo,
    }) as Device

const statuses = (
    entries: Record<UID, Record<string, ChannelValues>>,
): Map<UID, Map<string, ChannelValues>> =>
    new Map(
        Object.entries(entries).map(([uid, channels]) => [uid, new Map(Object.entries(channels))]),
    )

describe('isDimm', () => {
    it('matches the DDR5 and DDR4 module sensors', () => {
        expect(isDimm(device('a', DeviceType.HWMON, 'spd5118'))).toBe(true)
        expect(isDimm(device('b', DeviceType.HWMON, 'jc42'))).toBe(true)
    })

    it('rejects other hwmon devices and other device types', () => {
        expect(isDimm(device('a', DeviceType.HWMON, 'nct6798'))).toBe(false)
        expect(isDimm(device('b', DeviceType.HWMON))).toBe(false)
        expect(isDimm(device('c', DeviceType.CPU, 'spd5118'))).toBe(false)
        expect(isDimm({ uid: 'd', type: DeviceType.HWMON } as Device)).toBe(false)
    })
})

describe('liveLoad', () => {
    const devices = [
        device('cpu', DeviceType.CPU),
        device('gpu', DeviceType.GPU),
        device('dimm1', DeviceType.HWMON, 'spd5118'),
        device('dimm2', DeviceType.HWMON, 'spd5118'),
        device('board', DeviceType.HWMON, 'nct6798'),
    ]
    const current = statuses({
        cpu: { temp1: { temp: '71.4' }, temp2: { temp: '65.0' }, power1: { watts: '95.2' } },
        gpu: { temp1: { temp: '60' }, power1: { watts: '200' }, power2: { watts: '50' } },
        dimm1: { temp1: { temp: '46.5' } },
        dimm2: { temp1: { temp: '48.25' } },
        board: { temp1: { temp: '99' } },
    })

    it('takes the hottest temp and sums the watts of the targeted devices only', () => {
        expect(liveLoad(devices, current, isCpu)).toEqual({ tempMax: 71.4, watts: 95.2 })
        expect(liveLoad(devices, current, isGpu)).toEqual({ tempMax: 60, watts: 250 })
    })

    it('takes the hottest module across every DIMM', () => {
        expect(liveLoad(devices, current, isDimm)).toEqual({ tempMax: 48.25 })
    })

    it('is empty without a targeted device, a status, or a readable value', () => {
        expect(liveLoad([], current, isCpu)).toEqual({})
        expect(liveLoad(devices, new Map(), isCpu)).toEqual({})
        const unreadable = statuses({ cpu: { temp1: { temp: 'n/a' }, fan1: { rpm: '900' } } })
        expect(liveLoad(devices, unreadable, isCpu)).toEqual({})
    })
})

describe('formatLoad', () => {
    it('rounds and joins what is present', () => {
        expect(formatLoad({ tempMax: 71.4, watts: 95.2 })).toBe('71°C · 95W')
        expect(formatLoad({ tempMax: 48.5 })).toBe('49°C')
        expect(formatLoad({ watts: 12 }, { label: 'CPU' })).toBe('CPU 12W')
    })

    it('is empty, label included, when there is nothing to show', () => {
        expect(formatLoad({})).toBe('')
        expect(formatLoad({}, { label: 'CPU' })).toBe('')
    })
})

const labels = { dimm: 'DIMM', cpu: 'CPU' }

describe('formatRamLoad', () => {
    it('leads with the modules when they have a sensor', () => {
        expect(formatRamLoad({ tempMax: 48 }, { tempMax: 71, watts: 95 }, labels)).toBe(
            'DIMM 48°C · CPU 71°C · 95W',
        )
        expect(formatRamLoad({ tempMax: 48 }, {}, labels)).toBe('DIMM 48°C')
    })

    it('uses the labels it is given', () => {
        expect(formatRamLoad({ tempMax: 48 }, { tempMax: 71 }, { dimm: 'Модуль', cpu: 'ЦП' })).toBe(
            'Модуль 48°C · ЦП 71°C',
        )
    })

    it('stays on the CPU readout without a module sensor', () => {
        expect(formatRamLoad({}, { tempMax: 71, watts: 95 }, labels)).toBe('71°C · 95W')
        expect(formatRamLoad({}, {}, labels)).toBe('')
    })
})

describe('raisePeak', () => {
    it('starts at the first reading and only ever rises', () => {
        expect(raisePeak(undefined, 48)).toBe(48)
        expect(raisePeak(48, 52)).toBe(52)
        expect(raisePeak(52, 47)).toBe(52)
    })

    it('keeps the peak through a missing reading', () => {
        expect(raisePeak(52, undefined)).toBe(52)
        expect(raisePeak(undefined, undefined)).toBeUndefined()
    })
})

describe('trackedPeak', () => {
    it('rises while the test stays active', () => {
        expect(trackedPeak(80, 85, true, true)).toBe(85)
        expect(trackedPeak(80, 70, true, true)).toBe(80)
    })

    it('starts fresh when the test goes from idle to running', () => {
        expect(trackedPeak(90, 65, true, false)).toBe(65)
        expect(trackedPeak(90, undefined, true, false)).toBeUndefined()
    })

    it('keeps the peak of a test that has ended', () => {
        expect(trackedPeak(90, 65, false, true)).toBe(90)
        expect(trackedPeak(90, 65, false, false)).toBe(90)
    })
})
