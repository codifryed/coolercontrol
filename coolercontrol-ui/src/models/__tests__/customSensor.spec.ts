// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import { instanceToPlain, plainToInstance } from 'class-transformer'
import {
    CustomSensor,
    CustomSensorMetric,
    CustomSensorMixFunctionType,
    CustomSensorSourceData,
    CustomSensorType,
} from '../CustomSensor.ts'

// What goes over the wire: undefined members are dropped, as JSON does.
const wire = (value: unknown): unknown => JSON.parse(JSON.stringify(instanceToPlain(value)))

describe('CustomSensorSourceData', () => {
    it('puts a temperature source under temp_source', () => {
        const source = CustomSensorSourceData.of(CustomSensorMetric.Temp, 'dev-1', 'temp1', 3)

        expect(wire(source)).toStrictEqual({
            temp_source: { device_uid: 'dev-1', temp_name: 'temp1' },
            weight: 3,
        })
        expect(source.deviceUID).toBe('dev-1')
        expect(source.name).toBe('temp1')
    })

    it('puts a source of any other metric under channel_source', () => {
        for (const metric of [
            CustomSensorMetric.Duty,
            CustomSensorMetric.RPM,
            CustomSensorMetric.Freq,
            CustomSensorMetric.Watts,
        ]) {
            const source = CustomSensorSourceData.of(metric, 'dev-1', 'fan1')

            expect(wire(source)).toStrictEqual({
                channel_source: { device_uid: 'dev-1', channel_name: 'fan1' },
                weight: 1,
            })
            expect(source.deviceUID).toBe('dev-1')
            expect(source.name).toBe('fan1')
        }
    })
})

describe('CustomSensor', () => {
    it('reads a payload without a metric as a temperature sensor', () => {
        const sensor = plainToInstance(CustomSensor, {
            id: 'sensor1',
            cs_type: 'Mix',
            mix_function: 'Max',
            sources: [{ temp_source: { device_uid: 'dev-1', temp_name: 'temp1' }, weight: 1 }],
        })

        expect(sensor.metric).toBe(CustomSensorMetric.Temp)
        expect(sensor.sources[0]).toBeInstanceOf(CustomSensorSourceData)
        expect(sensor.sources[0].name).toBe('temp1')
    })

    it('round-trips a scaled channel sensor', () => {
        const payload = {
            id: 'sensor_3f9a1c2e',
            metric: 'RPM',
            cs_type: 'Offset',
            scale: 0.001,
            offset: 0,
            sources: [{ channel_source: { device_uid: 'dev-1', channel_name: 'fan1' }, weight: 1 }],
            children: [],
            parents: [],
        }

        const sensor = plainToInstance(CustomSensor, payload)

        expect(sensor.metric).toBe(CustomSensorMetric.RPM)
        expect(sensor.cs_type).toBe(CustomSensorType.Offset)
        expect(sensor.scale).toBe(0.001)
        expect(sensor.sources[0].deviceUID).toBe('dev-1')
        expect(sensor.sources[0].name).toBe('fan1')
        // The flat model always carries a mix function, which the daemon ignores here.
        expect(wire(sensor)).toStrictEqual({ ...payload, mix_function: 'Max' })
    })

    it('starts a new sensor as a temperature Mix', () => {
        const sensor = new CustomSensor('sensor_3f9a1c2e')

        expect(sensor.metric).toBe(CustomSensorMetric.Temp)
        expect(sensor.cs_type).toBe(CustomSensorType.Mix)
        expect(sensor.mix_function).toBe(CustomSensorMixFunctionType.Max)
    })
})
