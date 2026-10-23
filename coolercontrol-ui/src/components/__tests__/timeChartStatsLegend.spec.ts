// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import 'reflect-metadata'
import { describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import { createMemoryHistory, createRouter } from 'vue-router'
import en from '@/i18n/locales/en.ts'
import { DataType } from '@/models/Dashboard.ts'
import type { WindowLineStats, WindowStatsPayload } from '@/components/chartStats.ts'
import TimeChartStatsLegend from '@/components/TimeChartStatsLegend.vue'

const DEVICE = 'dev1'
const CUSTOM_SENSORS = 'cs1'

const devices = [
    {
        uid: DEVICE,
        type: 'Hwmon',
        info: {
            temps: new Map([['temp1', {}]]),
            channels: new Map([['fan1', { speed_options: { fixed_enabled: true } }]]),
        },
    },
    {
        uid: CUSTOM_SENSORS,
        type: 'CustomSensors',
        info: { temps: new Map([['sensor1', {}]]), channels: new Map() },
    },
]

vi.mock('@/stores/SettingsStore.ts', () => ({
    useSettingsStore: () => ({
        statsLegendScope: 'window',
        allUIDeviceSettings: new Map(),
        frequencyPrecision: 1,
        channelUnit: (): undefined => undefined,
        rpmUnit: (): string => 'rpm',
    }),
}))
vi.mock('@/stores/DeviceStore.ts', () => ({
    useDeviceStore: () => ({ allDevices: () => devices }),
}))
vi.mock('@/composables/useLifetimeStats.ts', () => ({
    useLifetimeStats: () => ({ displayOf: () => null }),
}))

const line = (
    deviceUID: string,
    channelName: string,
    dataType: DataType,
    seriesIndex: number,
): WindowLineStats => ({
    deviceUID,
    channelName,
    dataType,
    lineName: `${deviceUID}_${channelName}_${dataType}`,
    seriesIndex,
    color: '#ffffff',
    label: channelName,
    latest: 1,
    stats: null,
})

const payload: WindowStatsPayload = {
    lines: [
        line(DEVICE, 'temp1', DataType.TEMP, 1),
        line(DEVICE, 'fan1', DataType.RPM, 2),
        line(CUSTOM_SENSORS, 'sensor1', DataType.TEMP, 3),
    ],
    sampleRange: null,
    spanSeconds: 60,
    zoomed: false,
    scaleRanges: {},
}

async function legendLinks(): Promise<Array<string | undefined>> {
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [
            { path: '/', component: { template: '<div />' } },
            {
                path: '/monitoring/sensors/:deviceUID/:channelName',
                name: 'monitoring-sensor',
                component: { template: '<div />' },
            },
        ],
    })
    await router.push('/')
    const wrapper = mount(TimeChartStatsLegend, {
        props: { payload, rangeMinutes: 1 },
        global: {
            plugins: [router, createI18n({ legacy: false, locale: 'en', messages: { en } })],
            stubs: { HelpIcon: true, UiToggleGroup: true, SvgIcon: true },
        },
    })
    await flushPromises()
    return wrapper.findAll('tbody a').map((link) => link.attributes('href'))
}

describe('TimeChartStatsLegend', () => {
    it("links every line to its sensor's Monitoring chart", async () => {
        // Fans and custom sensors too: the legend lives in Monitoring, so it
        // keeps them on their chart rather than their Cooling or Devices page.
        expect(await legendLinks()).toEqual([
            `/monitoring/sensors/${DEVICE}/temp1`,
            `/monitoring/sensors/${DEVICE}/fan1`,
            `/monitoring/sensors/${CUSTOM_SENSORS}/sensor1`,
        ])
    })
})
