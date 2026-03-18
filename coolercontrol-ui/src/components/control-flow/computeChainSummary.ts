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

import type { Profile } from '@/models/Profile'
import { ProfileType } from '@/models/Profile'
import type { Device, UID } from '@/models/Device'
import { DeviceType } from '@/models/Device'
import type { DeviceUISettings } from '@/models/UISettings'

export interface ChainSummaryStep {
    type: 'profile' | 'tempSource'
    name: string
    subtype?: string
}

export interface ChainSummary {
    steps: ChainSummaryStep[]
    hasChain: boolean
}

export function computeChainSummary(
    profileUID: string | undefined,
    isManual: boolean,
    profiles: Profile[],
    allUIDeviceSettings: Map<string, DeviceUISettings>,
    allDevices: Device[],
): ChainSummary {
    const empty: ChainSummary = { steps: [], hasChain: false }
    if (isManual) return empty
    if (!profileUID || profileUID === '0') return empty

    const steps: ChainSummaryStep[] = []
    traceChain(profileUID, profiles, allUIDeviceSettings, allDevices, steps, 0)

    return { steps, hasChain: steps.length > 0 }
}

function traceChain(
    profileUID: UID,
    profiles: Profile[],
    allUIDeviceSettings: Map<string, DeviceUISettings>,
    allDevices: Device[],
    steps: ChainSummaryStep[],
    depth: number,
): void {
    if (depth > 2) return

    const profile = profiles.find((p) => p.uid === profileUID)
    if (!profile) return

    steps.push({
        type: 'profile',
        name: profile.name,
        subtype: profile.p_type,
    })

    if (profile.p_type === ProfileType.Graph && profile.temp_source) {
        const tempLabel = resolveTempLabel(
            profile.temp_source.device_uid,
            profile.temp_source.temp_name,
            allUIDeviceSettings,
            allDevices,
        )
        steps.push({
            type: 'tempSource',
            name: tempLabel,
        })
    } else if (profile.p_type === ProfileType.Overlay && profile.member_profile_uids.length > 0) {
        traceChain(
            profile.member_profile_uids[0],
            profiles,
            allUIDeviceSettings,
            allDevices,
            steps,
            depth + 1,
        )
    } else if (profile.p_type === ProfileType.Mix && profile.member_profile_uids.length > 0) {
        traceChain(
            profile.member_profile_uids[0],
            profiles,
            allUIDeviceSettings,
            allDevices,
            steps,
            depth + 1,
        )
    }
}

function resolveTempLabel(
    deviceUID: UID,
    tempName: string,
    allUIDeviceSettings: Map<string, DeviceUISettings>,
    allDevices: Device[],
): string {
    // Check if this is a custom sensor device
    const device = allDevices.find((d) => d.uid === deviceUID)
    if (device?.type === DeviceType.CUSTOM_SENSORS) {
        const csSettings = allUIDeviceSettings.get(deviceUID)
        return csSettings?.sensorsAndChannels.get(tempName)?.name ?? tempName
    }

    const deviceSettings = allUIDeviceSettings.get(deviceUID)
    return deviceSettings?.sensorsAndChannels.get(tempName)?.name ?? tempName
}
