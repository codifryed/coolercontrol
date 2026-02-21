/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import 'reflect-metadata'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { reactive, ref } from 'vue'
import { useOverviewGraph } from '../useOverviewGraph'
import { DeviceType } from '@/models/Device'

// --- Mock stores ---

const mockDevices = new Map<string, any>()
const mockUISettings = ref(new Map<string, any>())
const mockDaemonSettings = ref(new Map<string, any>())
const mockMenuOrder = ref<any[]>([])

vi.mock('@/stores/DeviceStore', () => ({
    useDeviceStore: () =>
        reactive({
            allDevices: () => mockDevices.values(),
            currentDeviceStatus: ref(new Map()),
            getREMSize: (multiplier: number) => multiplier * 16,
        }),
}))

vi.mock('@/stores/SettingsStore', () => ({
    useSettingsStore: () =>
        reactive({
            allUIDeviceSettings: mockUISettings,
            allDaemonDeviceSettings: mockDaemonSettings,
            menuOrder: mockMenuOrder,
        }),
}))

vi.mock('vue-router', () => ({
    useRouter: () => ({ push: vi.fn() }),
}))

// --- Helpers ---

function makeDevice(uid: string, name: string, type: DeviceType, channels: Record<string, any>) {
    const channelMap = new Map<string, any>()
    for (const [k, v] of Object.entries(channels)) {
        // Ensure lcd_modes and lighting_modes are always arrays (matching ChannelInfo defaults)
        channelMap.set(k, {
            lcd_modes: [],
            lighting_modes: [],
            ...v,
        })
    }
    return {
        uid,
        name,
        type,
        info: { channels: channelMap },
        status: { temps: [], channels: [] },
    }
}

function makeUIDeviceSettings(
    deviceName: string,
    channels: Record<string, { name: string; color: string }>,
    userColor?: string,
) {
    const sensorsAndChannels = new Map<string, any>()
    for (const [k, v] of Object.entries(channels)) {
        sensorsAndChannels.set(k, { name: v.name, color: v.color })
    }
    return { name: deviceName, sensorsAndChannels, userColor }
}

function makeDaemonDeviceSettings(channels: Record<string, any>) {
    const settings = new Map<string, any>()
    for (const [k, v] of Object.entries(channels)) {
        settings.set(k, v)
    }
    return { settings }
}

function clearMocks() {
    mockDevices.clear()
    mockUISettings.value = new Map()
    mockDaemonSettings.value = new Map()
    mockMenuOrder.value = []
}

// --- Tests ---

describe('useOverviewGraph', () => {
    beforeEach(() => {
        clearMocks()
    })

    it('returns empty nodes when no devices exist', () => {
        const { nodes } = useOverviewGraph()
        expect(nodes.value).toHaveLength(0)
    })

    it('skips CPU and CUSTOM_SENSORS devices', () => {
        mockDevices.set(
            'cpu1',
            makeDevice('cpu1', 'CPU', DeviceType.CPU, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDevices.set(
            'cs1',
            makeDevice('cs1', 'Custom Sensors', DeviceType.CUSTOM_SENSORS, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )

        const { nodes } = useOverviewGraph()
        expect(nodes.value).toHaveLength(0)
    })

    it('creates device label + fan nodes for a device with fans', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device A', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
                fan2: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings(
                'My Device',
                {
                    fan1: { name: 'Fan 1', color: '#ff0000' },
                    fan2: { name: 'Fan 2', color: '#0000ff' },
                },
                '#aa00aa',
            ),
        )

        const { nodes } = useOverviewGraph()

        const labelNodes = nodes.value.filter((n) => n.type === 'deviceLabel')
        const fanNodes = nodes.value.filter((n) => n.type === 'fanChannel')

        expect(labelNodes).toHaveLength(1)
        expect(labelNodes[0].data.deviceName).toBe('My Device')
        expect(labelNodes[0].data.deviceColor).toBe('#aa00aa')
        expect(labelNodes[0].data.channelCount).toBe(2)

        expect(fanNodes).toHaveLength(2)
        expect(fanNodes[0].data.channelLabel).toBe('Fan 1')
        expect(fanNodes[1].data.channelLabel).toBe('Fan 2')
    })

    it('skips devices with no controllable channels', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device A', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: false } },
            }),
        )

        const { nodes } = useOverviewGraph()
        expect(nodes.value).toHaveLength(0)
    })

    it('includes profileUID and isManual in fan node data', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
                fan2: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { speed_fixed: 50 },
                fan2: { profile_uid: 'p1' },
            }),
        )

        const { nodes } = useOverviewGraph()
        const fanNodes = nodes.value.filter((n) => n.type === 'fanChannel')

        const manualFan = fanNodes.find((n) => n.data.channelName === 'fan1')
        expect(manualFan?.data.isManual).toBe(true)
        expect(manualFan?.data.manualDuty).toBe(50)

        const profileFan = fanNodes.find((n) => n.data.channelName === 'fan2')
        expect(profileFan?.data.isManual).toBe(false)
        expect(profileFan?.data.profileUID).toBe('p1')
    })

    it('sorts devices by menuOrder', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device A', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDevices.set(
            'dev2',
            makeDevice('dev2', 'Device B', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockUISettings.value.set('dev1', makeUIDeviceSettings('Device A', {}))
        mockUISettings.value.set('dev2', makeUIDeviceSettings('Device B', {}))

        // Put dev2 before dev1 in menuOrder
        mockMenuOrder.value = [
            { id: 'dev2', children: [] },
            { id: 'dev1', children: [] },
        ]

        const { nodes } = useOverviewGraph()
        const labelNodes = nodes.value.filter((n) => n.type === 'deviceLabel')

        expect(labelNodes[0].data.deviceName).toBe('Device B')
        expect(labelNodes[1].data.deviceName).toBe('Device A')
    })

    it('sorts channels by menuOrder children', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
                fan2: { speed_options: { fixed_enabled: true } },
                fan3: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('Device', {
                fan1: { name: 'Fan 1', color: '#ff0000' },
                fan2: { name: 'Fan 2', color: '#00ff00' },
                fan3: { name: 'Fan 3', color: '#0000ff' },
            }),
        )

        // Order: fan3, fan1, fan2
        mockMenuOrder.value = [{ id: 'dev1', children: ['dev1_fan3', 'dev1_fan1', 'dev1_fan2'] }]

        const { nodes } = useOverviewGraph()
        const fanNodes = nodes.value.filter((n) => n.type === 'fanChannel')

        expect(fanNodes[0].data.channelLabel).toBe('Fan 3')
        expect(fanNodes[1].data.channelLabel).toBe('Fan 1')
        expect(fanNodes[2].data.channelLabel).toBe('Fan 2')
    })

    it('populates availableFans list', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('My Device', {
                fan1: { name: 'Fan 1', color: '#ff0000' },
            }),
        )

        const { availableFans } = useOverviewGraph()
        expect(availableFans.value).toHaveLength(1)
        expect(availableFans.value[0].key).toBe('dev1/fan1')
        expect(availableFans.value[0].label).toBe('My Device - Fan 1')
    })

    it('lays out fan nodes in rows of 3', () => {
        const channels: Record<string, any> = {}
        for (let i = 1; i <= 5; i++) {
            channels[`fan${i}`] = { speed_options: { fixed_enabled: true } }
        }
        mockDevices.set('dev1', makeDevice('dev1', 'Device', DeviceType.HWMON, channels))

        const { nodes } = useOverviewGraph()
        const fanNodes = nodes.value.filter((n) => n.type === 'fanChannel')

        expect(fanNodes).toHaveLength(5)
        // First row: 3 fans at x=0, 320, 640
        expect(fanNodes[0].position.x).toBe(0)
        expect(fanNodes[1].position.x).toBe(320)
        expect(fanNodes[2].position.x).toBe(640)
        // Second row: 2 fans at x=0, 320
        expect(fanNodes[3].position.x).toBe(0)
        expect(fanNodes[4].position.x).toBe(320)
        // Row 2 should be lower than row 1
        expect(fanNodes[3].position.y).toBeGreaterThan(fanNodes[0].position.y)
    })

    it('creates lcdChannel nodes for devices with lcd_modes', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                lcd1: { lcd_modes: [{ name: 'temp', frontend_name: 'Temp' }] },
            }),
        )
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings(
                'My Device',
                { lcd1: { name: 'LCD Screen', color: '#aabbcc' } },
                '#112233',
            ),
        )

        const { nodes } = useOverviewGraph()
        const lcdNodes = nodes.value.filter((n) => n.type === 'lcdChannel')
        expect(lcdNodes).toHaveLength(1)
        expect(lcdNodes[0].data.channelLabel).toBe('LCD Screen')
        expect(lcdNodes[0].data.channelColor).toBe('#aabbcc')
        expect(lcdNodes[0].data.deviceUID).toBe('dev1')
    })

    it('creates lightingChannel nodes for devices with lighting_modes', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                led1: {
                    lighting_modes: [
                        {
                            name: 'fixed',
                            frontend_name: 'Fixed',
                            min_colors: 1,
                            max_colors: 1,
                            speed_enabled: false,
                            backward_enabled: false,
                        },
                    ],
                },
            }),
        )
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('My Device', { led1: { name: 'LED Ring', color: '#ff00ff' } }),
        )

        const { nodes } = useOverviewGraph()
        const lightingNodes = nodes.value.filter((n) => n.type === 'lightingChannel')
        expect(lightingNodes).toHaveLength(1)
        expect(lightingNodes[0].data.channelLabel).toBe('LED Ring')
        expect(lightingNodes[0].data.channelColor).toBe('#ff00ff')
        expect(lightingNodes[0].data.deviceUID).toBe('dev1')
    })

    it('places lcd nodes below fan nodes within the same device group', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
                lcd1: { lcd_modes: [{ name: 'temp', frontend_name: 'Temp' }] },
            }),
        )

        const { nodes } = useOverviewGraph()
        const fanNode = nodes.value.find((n) => n.type === 'fanChannel')
        const lcdNode = nodes.value.find((n) => n.type === 'lcdChannel')

        expect(fanNode).toBeDefined()
        expect(lcdNode).toBeDefined()
        expect(lcdNode!.position.y).toBeGreaterThan(fanNode!.position.y)
    })

    it('uses default colors when no user settings', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )

        const { nodes } = useOverviewGraph()
        const labelNode = nodes.value.find((n) => n.type === 'deviceLabel')
        const fanNode = nodes.value.find((n) => n.type === 'fanChannel')

        expect(labelNode?.data.deviceColor).toBe('#568af2')
        expect(fanNode?.data.channelColor).toBe('#568af2')
    })
})
