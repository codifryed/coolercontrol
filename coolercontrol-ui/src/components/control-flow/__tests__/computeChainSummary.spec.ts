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
import { describe, it, expect } from 'vitest'
import { computeChainSummary } from '../computeChainSummary'
import { ProfileType } from '@/models/Profile'
import { DeviceType } from '@/models/Device'

function makeProfile(uid: string, name: string, pType: ProfileType, extra: any = {}) {
    return {
        uid,
        name,
        p_type: pType,
        speed_fixed: extra.speed_fixed,
        speed_profile: extra.speed_profile ?? [],
        temp_source: extra.temp_source,
        function_uid: extra.function_uid ?? '0',
        member_profile_uids: extra.member_profile_uids ?? [],
        mix_function_type: extra.mix_function_type,
        offset_profile: extra.offset_profile ?? [],
    } as any
}

function makeUISettings(channels: Record<string, { name: string; color?: string }>) {
    const sensorsAndChannels = new Map<string, any>()
    for (const [k, v] of Object.entries(channels)) {
        sensorsAndChannels.set(k, { name: v.name, color: v.color ?? '#000' })
    }
    return { sensorsAndChannels, name: 'Device' } as any
}

function makeDevice(uid: string, type: DeviceType) {
    return { uid, type, name: uid } as any
}

describe('computeChainSummary', () => {
    it('returns empty for manual fans', () => {
        const result = computeChainSummary('p1', true, [], new Map(), [])
        expect(result.hasChain).toBe(false)
        expect(result.steps).toHaveLength(0)
    })

    it('returns empty for undefined profileUID', () => {
        const result = computeChainSummary(undefined, false, [], new Map(), [])
        expect(result.hasChain).toBe(false)
    })

    it('returns empty for default profile (UID 0)', () => {
        const result = computeChainSummary('0', false, [], new Map(), [])
        expect(result.hasChain).toBe(false)
    })

    it('returns empty when profile not found', () => {
        const result = computeChainSummary('missing', false, [], new Map(), [])
        expect(result.hasChain).toBe(false)
    })

    it('traces a Graph profile with temp source', () => {
        const profiles = [
            makeProfile('p1', 'CPU Fan', ProfileType.Graph, {
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
            }),
        ]
        const uiSettings = new Map<string, any>()
        uiSettings.set('dev1', makeUISettings({ temp1: { name: 'CPU Temp' } }))
        const devices = [makeDevice('dev1', DeviceType.HWMON)]

        const result = computeChainSummary('p1', false, profiles, uiSettings, devices)

        expect(result.hasChain).toBe(true)
        expect(result.steps).toHaveLength(2)
        expect(result.steps[0]).toEqual({
            type: 'profile',
            name: 'CPU Fan',
            subtype: ProfileType.Graph,
        })
        expect(result.steps[1]).toEqual({
            type: 'tempSource',
            name: 'CPU Temp',
        })
    })

    it('traces a Fixed profile', () => {
        const profiles = [makeProfile('p1', 'Silent', ProfileType.Fixed, { speed_fixed: 30 })]

        const result = computeChainSummary('p1', false, profiles, new Map(), [])

        expect(result.hasChain).toBe(true)
        expect(result.steps).toHaveLength(1)
        expect(result.steps[0].name).toBe('Silent')
        expect(result.steps[0].subtype).toBe(ProfileType.Fixed)
    })

    it('traces an Overlay profile into its first member', () => {
        const profiles = [
            makeProfile('overlay1', 'My Overlay', ProfileType.Overlay, {
                member_profile_uids: ['graph1'],
            }),
            makeProfile('graph1', 'Graph Profile', ProfileType.Graph, {
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
            }),
        ]
        const uiSettings = new Map<string, any>()
        uiSettings.set('dev1', makeUISettings({ temp1: { name: 'GPU Temp' } }))

        const result = computeChainSummary('overlay1', false, profiles, uiSettings, [])

        expect(result.hasChain).toBe(true)
        expect(result.steps).toHaveLength(3)
        expect(result.steps[0].name).toBe('My Overlay')
        expect(result.steps[0].subtype).toBe(ProfileType.Overlay)
        expect(result.steps[1].name).toBe('Graph Profile')
        expect(result.steps[2].name).toBe('GPU Temp')
    })

    it('traces a Mix profile into its first member', () => {
        const profiles = [
            makeProfile('mix1', 'Mix Profile', ProfileType.Mix, {
                member_profile_uids: ['graph1', 'graph2'],
            }),
            makeProfile('graph1', 'Profile A', ProfileType.Graph, {
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
            }),
            makeProfile('graph2', 'Profile B', ProfileType.Graph, {
                temp_source: { device_uid: 'dev1', temp_name: 'temp2' },
            }),
        ]
        const uiSettings = new Map<string, any>()
        uiSettings.set('dev1', makeUISettings({ temp1: { name: 'CPU Temp' } }))

        const result = computeChainSummary('mix1', false, profiles, uiSettings, [])

        expect(result.hasChain).toBe(true)
        expect(result.steps).toHaveLength(3)
        expect(result.steps[0].name).toBe('Mix Profile')
        expect(result.steps[1].name).toBe('Profile A')
        expect(result.steps[2].name).toBe('CPU Temp')
    })

    it('respects depth limit of 2', () => {
        const profiles = [
            makeProfile('o1', 'Overlay 1', ProfileType.Overlay, {
                member_profile_uids: ['o2'],
            }),
            makeProfile('o2', 'Overlay 2', ProfileType.Overlay, {
                member_profile_uids: ['o3'],
            }),
            makeProfile('o3', 'Overlay 3', ProfileType.Overlay, {
                member_profile_uids: ['graph1'],
            }),
            makeProfile('graph1', 'Deep Graph', ProfileType.Graph, {
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
            }),
        ]

        const result = computeChainSummary('o1', false, profiles, new Map(), [])

        // depth 0: o1, depth 1: o2, depth 2: o3, depth 3: stops
        expect(result.steps).toHaveLength(3)
        expect(result.steps[2].name).toBe('Overlay 3')
    })

    it('uses temp_name as fallback when no UI settings', () => {
        const profiles = [
            makeProfile('p1', 'Fan', ProfileType.Graph, {
                temp_source: { device_uid: 'dev1', temp_name: 'temp1' },
            }),
        ]

        const result = computeChainSummary('p1', false, profiles, new Map(), [])

        expect(result.steps[1].name).toBe('temp1')
    })

    it('resolves custom sensor temp label', () => {
        const profiles = [
            makeProfile('p1', 'Fan', ProfileType.Graph, {
                temp_source: { device_uid: 'cs-dev', temp_name: 'cs1' },
            }),
        ]
        const uiSettings = new Map<string, any>()
        uiSettings.set('cs-dev', makeUISettings({ cs1: { name: 'Max CPU/GPU' } }))
        const devices = [makeDevice('cs-dev', DeviceType.CUSTOM_SENSORS)]

        const result = computeChainSummary('p1', false, profiles, uiSettings, devices)

        expect(result.steps[1].name).toBe('Max CPU/GPU')
    })
})
