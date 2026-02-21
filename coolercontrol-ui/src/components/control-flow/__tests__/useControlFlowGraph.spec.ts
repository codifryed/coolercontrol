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
import { useControlFlowGraph, COLUMN_GAP } from '../useControlFlowGraph'
import { ProfileType } from '@/models/Profile'
import { FunctionType } from '@/models/Profile'
import { DeviceType } from '@/models/Device'

// --- Mock stores ---
// Using reactive() to auto-unwrap refs, matching Pinia store behavior

const mockDevices = new Map<string, any>()
const mockUISettings = ref(new Map<string, any>())
const mockDaemonSettings = ref(new Map<string, any>())
const mockProfiles = ref<any[]>([])
const mockFunctions = ref<any[]>([])
const mockCurrentDeviceStatus = ref(new Map<string, Map<string, any>>())

vi.mock('@/stores/DeviceStore', () => ({
    useDeviceStore: () =>
        reactive({
            allDevices: () => mockDevices.values(),
            currentDeviceStatus: mockCurrentDeviceStatus,
        }),
}))

vi.mock('@/stores/SettingsStore', () => ({
    useSettingsStore: () =>
        reactive({
            allUIDeviceSettings: mockUISettings,
            allDaemonDeviceSettings: mockDaemonSettings,
            profiles: mockProfiles,
            functions: mockFunctions,
            getCustomSensors: vi.fn().mockResolvedValue([]),
        }),
}))

vi.mock('vue-router', () => ({
    useRouter: () => ({ push: vi.fn() }),
}))

// --- Helpers ---

function makeDevice(uid: string, name: string, type: DeviceType, channels: Record<string, any>) {
    const channelMap = new Map<string, any>()
    for (const [k, v] of Object.entries(channels)) {
        channelMap.set(k, v)
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
) {
    const sensorsAndChannels = new Map<string, any>()
    for (const [k, v] of Object.entries(channels)) {
        sensorsAndChannels.set(k, { name: v.name, color: v.color })
    }
    return { name: deviceName, sensorsAndChannels }
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
    mockProfiles.value = []
    mockFunctions.value = []
    mockCurrentDeviceStatus.value = new Map()
}

// --- Tests ---

describe('useControlFlowGraph', () => {
    beforeEach(() => {
        clearMocks()
    })

    it('returns empty graph when no devices exist', () => {
        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)
        expect(nodes.value).toHaveLength(0)
        expect(edges.value).toHaveLength(0)
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

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes } = useControlFlowGraph(selectedFanKey)
        expect(nodes.value).toHaveLength(0)
    })

    it('skips non-controllable channels', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: false } },
                fan2: { speed_options: null },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes } = useControlFlowGraph(selectedFanKey)
        expect(nodes.value).toHaveLength(0)
    })

    it('creates fan node with default profile when no daemon settings', () => {
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
        mockProfiles.value = [
            {
                uid: '0',
                p_type: ProfileType.Default,
                name: 'Default Profile',
                function_uid: '0',
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        const fanNodes = nodes.value.filter((n) => n.type === 'fanChannel')
        const profileNodes = nodes.value.filter((n) => n.type === 'profile')
        expect(fanNodes).toHaveLength(1)
        expect(fanNodes[0].data.channelLabel).toBe('Fan 1')
        expect(profileNodes).toHaveLength(1)
        expect(profileNodes[0].data.profileType).toBe(ProfileType.Default)
        expect(edges.value).toHaveLength(1)
    })

    it('creates manual fan node with no profile chain', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { speed_fixed: 50 },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        expect(nodes.value).toHaveLength(1)
        expect(nodes.value[0].data.isManual).toBe(true)
        expect(nodes.value[0].data.manualDuty).toBe(50)
        expect(edges.value).toHaveLength(0)
    })

    it('creates graph profile chain with temp source', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { profile_uid: 'p1' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'p1',
                p_type: ProfileType.Graph,
                name: 'Graph Profile',
                function_uid: 'f1',
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]
        mockFunctions.value = [{ uid: 'f1', name: 'Standard Fn', f_type: FunctionType.Standard }]
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('My Device', {
                fan1: { name: 'Fan 1', color: '#ff0000' },
                temp1: { name: 'CPU Temp', color: '#00ff00' },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        const fanNodes = nodes.value.filter((n) => n.type === 'fanChannel')
        const profileNodes = nodes.value.filter((n) => n.type === 'profile')
        const tempNodes = nodes.value.filter((n) => n.type === 'tempSource')

        expect(fanNodes).toHaveLength(1)
        expect(profileNodes).toHaveLength(1)
        expect(tempNodes).toHaveLength(1)
        expect(profileNodes[0].data.functionName).toBe('Standard Fn')
        expect(profileNodes[0].data.functionType).toBe(FunctionType.Standard)
        expect(tempNodes[0].data.tempLabel).toBe('CPU Temp')
        expect(edges.value).toHaveLength(2) // fan←profile, profile←temp
    })

    it('creates fixed profile chain without temp source', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { profile_uid: 'p1' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'p1',
                p_type: ProfileType.Fixed,
                name: 'Fixed 50%',
                function_uid: '0',
                speed_fixed: 50,
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        expect(nodes.value).toHaveLength(2) // fan + profile
        const profileNode = nodes.value.find((n) => n.type === 'profile')
        expect(profileNode?.data.speedFixed).toBe(50)
        expect(edges.value).toHaveLength(1)
    })

    it('expands mix profile members', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { profile_uid: 'mix1' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'mix1',
                p_type: ProfileType.Mix,
                name: 'Mix Profile',
                function_uid: '0',
                mix_function_type: 'Max',
                member_profile_uids: ['p1', 'p2'],
                speed_profile: [],
                offset_profile: [],
            },
            {
                uid: 'p1',
                p_type: ProfileType.Graph,
                name: 'Graph A',
                function_uid: '0',
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
            {
                uid: 'p2',
                p_type: ProfileType.Fixed,
                name: 'Fixed B',
                function_uid: '0',
                speed_fixed: 30,
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('Device', {
                fan1: { name: 'Fan 1', color: '#ff0000' },
                temp1: { name: 'Temp 1', color: '#00ff00' },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        // fan + mix + graph member + fixed member + temp
        expect(nodes.value).toHaveLength(5)
        // fan←mix, mix←p1, mix←p2, p1←temp
        expect(edges.value).toHaveLength(4)
    })

    it('expands overlay profile base', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { profile_uid: 'overlay1' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'overlay1',
                p_type: ProfileType.Overlay,
                name: 'Overlay Profile',
                function_uid: '0',
                member_profile_uids: ['base1'],
                speed_profile: [],
                offset_profile: [
                    [0, 5],
                    [100, 10],
                ],
            },
            {
                uid: 'base1',
                p_type: ProfileType.Graph,
                name: 'Base Graph',
                function_uid: '0',
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('Device', {
                fan1: { name: 'Fan 1', color: '#ff0000' },
                temp1: { name: 'Temp 1', color: '#00ff00' },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        // fan + overlay + base graph + temp
        expect(nodes.value).toHaveLength(4)
        // fan←overlay, overlay←base, base←temp
        expect(edges.value).toHaveLength(3)
    })

    it('deduplicates shared profiles', () => {
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
                fan1: { profile_uid: 'p1' },
                fan2: { profile_uid: 'p1' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'p1',
                p_type: ProfileType.Fixed,
                name: 'Shared Fixed',
                function_uid: '0',
                speed_fixed: 50,
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        // 2 fans + 1 shared profile
        expect(nodes.value).toHaveLength(3)
        // fan1←p1, fan2←p1
        expect(edges.value).toHaveLength(2)
    })

    it('filters to selected fan subgraph', () => {
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
                fan1: { profile_uid: 'p1' },
                fan2: { profile_uid: 'p2' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'p1',
                p_type: ProfileType.Fixed,
                name: 'Profile 1',
                function_uid: '0',
                speed_fixed: 50,
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
            {
                uid: 'p2',
                p_type: ProfileType.Fixed,
                name: 'Profile 2',
                function_uid: '0',
                speed_fixed: 75,
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]

        const selectedFanKey = ref<string | undefined>('dev1/fan1')
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        // Only fan1 + p1
        expect(nodes.value).toHaveLength(2)
        expect(edges.value).toHaveLength(1)
        expect(nodes.value.some((n) => n.id === 'fan::dev1::fan1')).toBe(true)
        expect(nodes.value.some((n) => n.id === 'fan::dev1::fan2')).toBe(false)
    })

    it('filters to single fan even when profiles are shared', () => {
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
                fan1: { profile_uid: 'shared' },
                fan2: { profile_uid: 'shared' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'shared',
                p_type: ProfileType.Fixed,
                name: 'Shared Profile',
                function_uid: '0',
                speed_fixed: 50,
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]

        const selectedFanKey = ref<string | undefined>('dev1/fan1')
        const { nodes, edges } = useControlFlowGraph(selectedFanKey)

        // Only fan1 + shared profile (not fan2)
        expect(nodes.value).toHaveLength(2)
        expect(edges.value).toHaveLength(1)
        expect(nodes.value.some((n) => n.id === 'fan::dev1::fan1')).toBe(true)
        expect(nodes.value.some((n) => n.id === 'fan::dev1::fan2')).toBe(false)
    })

    it('positions fan nodes at column 0 (leftmost)', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes } = useControlFlowGraph(selectedFanKey)

        const fanNode = nodes.value.find((n) => n.type === 'fanChannel')
        expect(fanNode?.position.x).toBe(0)
    })

    it('populates availableFans list', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
                fan2: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('My Device', {
                fan1: { name: 'Fan 1', color: '#ff0000' },
                fan2: { name: 'Fan 2', color: '#0000ff' },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { availableFans } = useControlFlowGraph(selectedFanKey)

        expect(availableFans.value).toHaveLength(2)
        expect(availableFans.value[0].key).toBe('dev1/fan1')
        expect(availableFans.value[0].label).toBe('My Device - Fan 1')
    })

    it('places profile types in fixed columns by subtype', () => {
        // Overlay → col 1, Graph → col 3, TempSource → col 5
        // With column collapsing: populated [0,1,3,5] → X: [0, GAP, 2*GAP, 3*GAP]
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { profile_uid: 'overlay1' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'overlay1',
                p_type: ProfileType.Overlay,
                name: 'Overlay',
                function_uid: '0',
                member_profile_uids: ['graph1'],
                speed_profile: [],
                offset_profile: [[0, 5]],
            },
            {
                uid: 'graph1',
                p_type: ProfileType.Graph,
                name: 'Graph',
                function_uid: '0',
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('Device', {
                temp1: { name: 'Temp', color: '#00ff00' },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes } = useControlFlowGraph(selectedFanKey)

        const fan = nodes.value.find((n) => n.type === 'fanChannel')
        const overlay = nodes.value.find(
            (n) => n.type === 'profile' && n.data.profileType === ProfileType.Overlay,
        )
        const graph = nodes.value.find(
            (n) => n.type === 'profile' && n.data.profileType === ProfileType.Graph,
        )
        const temp = nodes.value.find((n) => n.type === 'tempSource')

        // Empty columns (Mix, CustomSensor) collapsed — sequential X positions
        expect(fan?.position.x).toBe(0)
        expect(overlay?.position.x).toBe(1 * COLUMN_GAP)
        expect(graph?.position.x).toBe(2 * COLUMN_GAP)
        expect(temp?.position.x).toBe(3 * COLUMN_GAP)
    })

    it('places mix profile in column 2 and its members in column 3', () => {
        // Mix → col 2, Graph → col 3, TempSource → col 5
        // With column collapsing: populated [0,2,3,5] → X: [0, GAP, 2*GAP, 3*GAP]
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { profile_uid: 'mix1' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'mix1',
                p_type: ProfileType.Mix,
                name: 'Mix',
                function_uid: '0',
                mix_function_type: 'Max',
                member_profile_uids: ['graph1'],
                speed_profile: [],
                offset_profile: [],
            },
            {
                uid: 'graph1',
                p_type: ProfileType.Graph,
                name: 'Graph',
                function_uid: '0',
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('Device', {
                temp1: { name: 'Temp', color: '#00ff00' },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes } = useControlFlowGraph(selectedFanKey)

        const mix = nodes.value.find(
            (n) => n.type === 'profile' && n.data.profileType === ProfileType.Mix,
        )
        const graph = nodes.value.find(
            (n) => n.type === 'profile' && n.data.profileType === ProfileType.Graph,
        )
        const temp = nodes.value.find((n) => n.type === 'tempSource')

        // Empty columns (Overlay, CustomSensor) collapsed — sequential X positions
        expect(mix?.position.x).toBe(1 * COLUMN_GAP)
        expect(graph?.position.x).toBe(2 * COLUMN_GAP)
        expect(temp?.position.x).toBe(3 * COLUMN_GAP)
    })

    it('does not show function info for default function (uid=0)', () => {
        mockDevices.set(
            'dev1',
            makeDevice('dev1', 'Device', DeviceType.HWMON, {
                fan1: { speed_options: { fixed_enabled: true } },
            }),
        )
        mockDaemonSettings.value.set(
            'dev1',
            makeDaemonDeviceSettings({
                fan1: { profile_uid: 'p1' },
            }),
        )
        mockProfiles.value = [
            {
                uid: 'p1',
                p_type: ProfileType.Graph,
                name: 'Graph Profile',
                function_uid: '0',
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
                member_profile_uids: [],
                speed_profile: [],
                offset_profile: [],
            },
        ]
        mockUISettings.value.set(
            'dev1',
            makeUIDeviceSettings('Device', {
                temp1: { name: 'Temp', color: '#00ff00' },
            }),
        )

        const selectedFanKey = ref<string | undefined>(undefined)
        const { nodes } = useControlFlowGraph(selectedFanKey)

        const profileNode = nodes.value.find((n) => n.type === 'profile')
        expect(profileNode?.data.functionName).toBeUndefined()
    })
})
