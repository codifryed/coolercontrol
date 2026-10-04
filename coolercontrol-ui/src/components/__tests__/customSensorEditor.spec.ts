// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Device.ts uses class-transformer decorators which need the metadata polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import { type Device, DeviceType } from '@/models/Device.ts'
import { CustomSensor, CustomSensorMetric, CustomSensorType } from '@/models/CustomSensor.ts'
import {
    availableSources,
    canSave,
    type EditorState,
    isUsableScale,
    liveValue,
    newCustomSensorId,
    offsetLimit,
    type SourceLabels,
} from '@/components/customSensorEditor.ts'

const hashes = (...values: string[]): (() => string) => {
    let next = 0
    return () => values[Math.min(next++, values.length - 1)]
}

describe('newCustomSensorId', () => {
    it('prefixes an eight character hash', () => {
        expect(newCustomSensorId(new Set())).toMatch(/^sensor_[0-9a-f]{8}$/)
    })

    it('does not follow the numbered ids of existing sensors', () => {
        const taken = new Set(['sensor1', 'sensor2', 'sensor10', 'Auto Delta temp1 temp2'])
        expect(newCustomSensorId(taken, hashes('3f9a1c2e'))).toBe('sensor_3f9a1c2e')
    })

    it('draws again when the id is taken', () => {
        const taken = new Set(['sensor_aaaaaaaa'])
        expect(newCustomSensorId(taken, hashes('aaaaaaaa', 'bbbbbbbb'))).toBe('sensor_bbbbbbbb')
    })

    it('gives up when every draw is taken', () => {
        const taken = new Set(['sensor_aaaaaaaa'])
        expect(() => newCustomSensorId(taken, hashes('aaaaaaaa'))).toThrow()
    })
})

interface FakeChannel {
    name: string
    duty?: number
    rpm?: number
    freq?: number
    watts?: number
}

function fakeDevice(
    uid: string,
    type: DeviceType,
    temps: Record<string, number>,
    channels: FakeChannel[] = [],
): Device {
    return {
        uid,
        type,
        info: {},
        status: {
            temps: Object.entries(temps).map(([name, temp]) => ({ name, temp })),
            channels,
        },
    } as unknown as Device
}

// Every temp and channel is labelled "<name> label" in a fixed color.
const labelsOf = (deviceUID: string): SourceLabels => ({
    name: `Device ${deviceUID}`,
    sensorsAndChannels: new Proxy(new Map(), {
        get: (_, property) =>
            property === 'get'
                ? (name: string) => ({ name: `${name} label`, color: '#fff' })
                : undefined,
    }),
})

const sensor = (
    id: string,
    metric: CustomSensorMetric,
    relations: { children?: string[]; parents?: string[] } = {},
): CustomSensor => {
    const custom = new CustomSensor(id)
    custom.metric = metric
    custom.children = relations.children ?? []
    custom.parents = relations.parents ?? []
    return custom
}

const names = (groups: ReturnType<typeof availableSources>): string[] =>
    groups.flatMap((group) => group.sources.map((source) => `${group.deviceUID}/${source.name}`))

describe('availableSources', () => {
    const board = fakeDevice('board', DeviceType.HWMON, { temp1: 41.26 }, [
        { name: 'fan1', duty: 40, rpm: 1200 },
        { name: 'flow', rpm: 85 },
    ])
    const cpu = fakeDevice('cpu', DeviceType.CPU, { core: 55 }, [
        { name: 'load', duty: 12 },
        { name: 'freq', freq: 3600 },
        { name: 'power', watts: 65.52 },
    ])
    const edited = { id: 'new', parents: [] }

    it('lists temps for a temperature sensor', () => {
        const groups = availableSources([board, cpu], CustomSensorMetric.Temp, [], edited, labelsOf)

        expect(names(groups)).toEqual(['board/temp1', 'cpu/core'])
        expect(groups[0].deviceName).toBe('Device board')
        expect(groups[0].sources[0]).toEqual({
            deviceUID: 'board',
            name: 'temp1',
            frontendName: 'temp1 label',
            lineColor: '#fff',
            weight: 1,
            value: '41.3',
        })
    })

    it('lists only the channels reporting the metric', () => {
        const list = (metric: CustomSensorMetric) =>
            names(availableSources([board, cpu], metric, [], edited, labelsOf))

        // A fan reports both duty and rpm, a load only duty, a flow only rpm.
        expect(list(CustomSensorMetric.Duty)).toEqual(['board/fan1', 'cpu/load'])
        expect(list(CustomSensorMetric.RPM)).toEqual(['board/fan1', 'board/flow'])
        expect(list(CustomSensorMetric.Freq)).toEqual(['cpu/freq'])
        expect(list(CustomSensorMetric.Watts)).toEqual(['cpu/power'])
    })

    it('formats each metric as the app shows it', () => {
        const value = (metric: CustomSensorMetric) =>
            availableSources([cpu], metric, [], edited, labelsOf)[0].sources[0].value

        expect(value(CustomSensorMetric.Duty)).toBe('12')
        expect(value(CustomSensorMetric.Freq)).toBe('3600')
        expect(value(CustomSensorMetric.Watts)).toBe('65.5')
    })

    it('skips devices without info, labels or matching sources', () => {
        const bare = { ...board, info: undefined } as unknown as Device
        const unlabelled = (uid: string) => (uid === 'cpu' ? undefined : labelsOf(uid))

        expect(availableSources([bare], CustomSensorMetric.Temp, [], edited, labelsOf)).toEqual([])
        expect(
            names(availableSources([board, cpu], CustomSensorMetric.Temp, [], edited, unlabelled)),
        ).toEqual(['board/temp1'])
        expect(availableSources([board], CustomSensorMetric.Freq, [], edited, labelsOf)).toEqual([])
    })

    describe('other custom sensors', () => {
        const custom = fakeDevice('cs', DeviceType.CUSTOM_SENSORS, { t_free: 40, t_parent: 50 }, [
            { name: 'rpm_free', rpm: 438 },
            { name: 'self', rpm: 100 },
        ])
        const sensors = [
            sensor('t_free', CustomSensorMetric.Temp),
            sensor('t_parent', CustomSensorMetric.Temp, { children: ['t_free'] }),
            sensor('rpm_free', CustomSensorMetric.RPM),
            sensor('self', CustomSensorMetric.RPM),
        ]

        it('offers same-metric sensors that have no children, never the sensor itself', () => {
            const self = { id: 'self', parents: [] }

            expect(
                names(availableSources([custom], CustomSensorMetric.RPM, sensors, self, labelsOf)),
            ).toEqual(['cs/rpm_free'])
            expect(
                names(availableSources([custom], CustomSensorMetric.Temp, sensors, self, labelsOf)),
            ).toEqual(['cs/t_free'])
        })

        it('offers none to a sensor that is already a child', () => {
            const child = { id: 'self', parents: ['some_parent'] }

            expect(
                availableSources([custom], CustomSensorMetric.RPM, sensors, child, labelsOf),
            ).toEqual([])
        })

        it('skips a sensor the daemon does not list', () => {
            expect(
                availableSources([custom], CustomSensorMetric.RPM, [], edited, labelsOf),
            ).toEqual([])
        })
    })
})

describe('liveValue', () => {
    it('picks the value of the metric among a channel’s current values', () => {
        const fan = { duty: '40', rpm: '1200' }

        expect(liveValue(CustomSensorMetric.Duty, fan)).toBe('40')
        expect(liveValue(CustomSensorMetric.RPM, fan)).toBe('1200')
        expect(liveValue(CustomSensorMetric.Temp, { temp: '41.3' })).toBe('41.3')
        expect(liveValue(CustomSensorMetric.Freq, { freq: '3600' })).toBe('3600')
        expect(liveValue(CustomSensorMetric.Watts, { watts: '65.5' })).toBe('65.5')
    })

    it('has nothing for a value the channel does not report', () => {
        expect(liveValue(CustomSensorMetric.Watts, { duty: '40' })).toBeUndefined()
        expect(liveValue(CustomSensorMetric.RPM, undefined)).toBeUndefined()
    })
})

describe('offsetLimit', () => {
    it('keeps the temperature range and widens the others', () => {
        expect(offsetLimit(CustomSensorMetric.Temp)).toBe(100)
        for (const metric of [
            CustomSensorMetric.Duty,
            CustomSensorMetric.RPM,
            CustomSensorMetric.Freq,
            CustomSensorMetric.Watts,
        ]) {
            expect(offsetLimit(metric)).toBe(1_000_000)
        }
    })
})

describe('isUsableScale', () => {
    it('takes any finite factor inside the bounds, on either sign', () => {
        for (const scale of [1, -1, 0.001, 0.000001, -0.000001, 1_000_000, -1_000_000]) {
            expect(isUsableScale(scale)).toBe(true)
        }
    })

    it('refuses zero, values outside the bounds and non-numbers', () => {
        for (const scale of [0, 0.0000009, 1_000_001, NaN, Infinity, null, undefined]) {
            expect(isUsableScale(scale)).toBe(false)
        }
    })
})

describe('canSave', () => {
    const state = (overrides: Partial<EditorState>): EditorState => ({
        type: CustomSensorType.Mix,
        metric: CustomSensorMetric.Temp,
        mixSourceCount: 0,
        hasSingleSource: false,
        filePath: '',
        scale: 1,
        offset: 0,
        timeWindowSeconds: 10,
        ...overrides,
    })

    it('needs at least one source for a Mix', () => {
        expect(canSave(state({ mixSourceCount: 0 }))).toBe(false)
        expect(canSave(state({ mixSourceCount: 1 }))).toBe(true)
    })

    it('needs a path for a File', () => {
        const file = (filePath: string | null | undefined) =>
            canSave(state({ type: CustomSensorType.File, filePath }))

        expect(file('')).toBe(false)
        expect(file('   ')).toBe(false)
        expect(file(null)).toBe(false)
        expect(file(undefined)).toBe(false)
        expect(file('/tmp/value')).toBe(true)
    })

    it('needs a source, a usable scale and an offset in range for Scale & Offset', () => {
        const scaled = (overrides: Partial<EditorState>) =>
            canSave(state({ type: CustomSensorType.Offset, hasSingleSource: true, ...overrides }))

        expect(scaled({})).toBe(true)
        expect(scaled({ hasSingleSource: false })).toBe(false)
        expect(scaled({ scale: 0 })).toBe(false)
        expect(scaled({ scale: null })).toBe(false)
        expect(scaled({ offset: null })).toBe(false)
        expect(scaled({ offset: -100 })).toBe(true)
        expect(scaled({ offset: 100.5 })).toBe(false)
        expect(scaled({ metric: CustomSensorMetric.RPM, offset: 100.5 })).toBe(true)
        expect(scaled({ metric: CustomSensorMetric.RPM, offset: 1_000_001 })).toBe(false)
    })

    it('needs a source and a window of 1 to 300 seconds for the smoothing types', () => {
        for (const type of [CustomSensorType.TimeAverage, CustomSensorType.ExponentialMovingAvg]) {
            const smoothed = (overrides: Partial<EditorState>) =>
                canSave(state({ type, hasSingleSource: true, ...overrides }))

            expect(smoothed({})).toBe(true)
            expect(smoothed({ hasSingleSource: false })).toBe(false)
            expect(smoothed({ timeWindowSeconds: 0 })).toBe(false)
            expect(smoothed({ timeWindowSeconds: 301 })).toBe(false)
            expect(smoothed({ timeWindowSeconds: null })).toBe(false)
            expect(smoothed({ timeWindowSeconds: 300 })).toBe(true)
        }
    })
})
