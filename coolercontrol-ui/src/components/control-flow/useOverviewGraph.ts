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

import { computed } from 'vue'
import type { Node } from '@vue-flow/core'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { DeviceType } from '@/models/Device'
import type { FanNodeData, FanOption } from './useControlFlowGraph'

export interface DeviceLabelNodeData {
    deviceName: string
    deviceColor: string
    channelCount: number
}

export interface LcdChannelNodeData {
    deviceUID: string
    channelName: string
    channelLabel: string
    channelColor: string
    deviceLabel: string
    currentMode?: string
}

export interface LightingChannelNodeData {
    deviceUID: string
    channelName: string
    channelLabel: string
    channelColor: string
    deviceLabel: string
    currentMode?: string
}

const deviceStore = useDeviceStore()
const FANS_PER_ROW = 3
const COL_GAP = deviceStore.getREMSize(20)
const ROW_GAP = deviceStore.getREMSize(10)
const INTER_TYPE_GAP = deviceStore.getREMSize(8)
const DEVICE_LABEL_HEIGHT = deviceStore.getREMSize(3.5)
const GROUP_GAP = 0

export function useOverviewGraph() {
    const deviceStore = useDeviceStore()
    const settingsStore = useSettingsStore()

    const availableFans = computed<FanOption[]>(() => {
        const fans: FanOption[] = []
        for (const device of deviceStore.allDevices()) {
            if (device.type === DeviceType.CUSTOM_SENSORS || device.type === DeviceType.CPU)
                continue
            if (device.info == null) continue
            const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)
            for (const [channelName, channelInfo] of device.info.channels.entries()) {
                if (!channelInfo.speed_options?.fixed_enabled) continue
                const channelLabel =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.name ?? channelName
                const deviceLabel = deviceSettings?.name ?? device.name
                fans.push({
                    key: `${device.uid}/${channelName}`,
                    label: `${deviceLabel} - ${channelLabel}`,
                })
            }
        }
        return fans
    })

    const nodes = computed<Node[]>(() => {
        const result: Node[] = []

        // Get ordered devices using menuOrder
        const allDevices = [...deviceStore.allDevices()].filter(
            (d) =>
                d.type !== DeviceType.CUSTOM_SENSORS && d.type !== DeviceType.CPU && d.info != null,
        )

        const getDeviceIndex = (uid: string) => {
            const idx = settingsStore.menuOrder.findIndex((m) => m.id === uid)
            return idx >= 0 ? idx : Number.MAX_SAFE_INTEGER
        }
        allDevices.sort((a, b) => getDeviceIndex(a.uid) - getDeviceIndex(b.uid))

        let currentY = 0

        for (const device of allDevices) {
            const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)
            const daemonSettings = settingsStore.allDaemonDeviceSettings.get(device.uid)
            const deviceName = deviceSettings?.name ?? device.name
            const deviceColor = deviceSettings?.userColor ?? '#568af2'

            const menuItem = settingsStore.menuOrder.find((m) => m.id === device.uid)
            const getChildIndex = (channelName: string) => {
                if (!menuItem?.children?.length) return Number.MAX_SAFE_INTEGER
                const idx = menuItem.children.indexOf(`${device.uid}_${channelName}`)
                return idx >= 0 ? idx : Number.MAX_SAFE_INTEGER
            }

            // Collect each channel type
            const fanChannels: string[] = []
            const lcdChannels: string[] = []
            const lightingChannels: string[] = []
            for (const [channelName, channelInfo] of device.info!.channels.entries()) {
                if (channelInfo.speed_options?.fixed_enabled) fanChannels.push(channelName)
                if (channelInfo.lcd_modes.length > 0) lcdChannels.push(channelName)
                if (channelInfo.lighting_modes.length > 0) lightingChannels.push(channelName)
            }

            if (fanChannels.length + lcdChannels.length + lightingChannels.length === 0) continue

            // Sort each type by menuOrder children
            fanChannels.sort((a, b) => getChildIndex(a) - getChildIndex(b))
            lcdChannels.sort((a, b) => getChildIndex(a) - getChildIndex(b))
            lightingChannels.sort((a, b) => getChildIndex(a) - getChildIndex(b))

            // Label spans the widest row across all channel types
            const maxColsUsed = Math.max(
                Math.min(fanChannels.length, FANS_PER_ROW),
                Math.min(lcdChannels.length, FANS_PER_ROW),
                Math.min(lightingChannels.length, FANS_PER_ROW),
            )
            result.push({
                id: `device-label::${device.uid}`,
                type: 'deviceLabel',
                position: { x: 0, y: currentY },
                data: {
                    deviceName,
                    deviceColor,
                    channelCount: maxColsUsed,
                } satisfies DeviceLabelNodeData,
            })
            currentY += DEVICE_LABEL_HEIGHT

            // Fan nodes
            for (let i = 0; i < fanChannels.length; i++) {
                const channelName = fanChannels[i]
                const channelLabel =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.name ?? channelName
                const channelColor =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.color ?? '#568af2'
                const channelSetting = daemonSettings?.settings.get(channelName)
                result.push({
                    id: `fan::${device.uid}::${channelName}`,
                    type: 'fanChannel',
                    position: {
                        x: (i % FANS_PER_ROW) * COL_GAP,
                        y: currentY + Math.floor(i / FANS_PER_ROW) * ROW_GAP,
                    },
                    data: {
                        deviceUID: device.uid,
                        channelName,
                        channelLabel,
                        channelColor,
                        deviceLabel: deviceName,
                        isManual: channelSetting?.speed_fixed != null,
                        manualDuty: channelSetting?.speed_fixed,
                        profileUID: channelSetting?.profile_uid,
                    } satisfies FanNodeData,
                })
            }
            if (fanChannels.length > 0) {
                const hasMore = lcdChannels.length > 0 || lightingChannels.length > 0
                const lastRowGap = hasMore ? INTER_TYPE_GAP : ROW_GAP
                currentY +=
                    (Math.ceil(fanChannels.length / FANS_PER_ROW) - 1) * ROW_GAP + lastRowGap
            }

            // LCD nodes
            for (let i = 0; i < lcdChannels.length; i++) {
                const channelName = lcdChannels[i]
                const channelLabel =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.name ?? channelName
                const channelColor =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.color ?? '#568af2'
                const channelSetting = daemonSettings?.settings.get(channelName)
                const lcdModeName = channelSetting?.lcd?.mode
                const lcdFrontendName = lcdModeName
                    ? device
                          .info!.channels.get(channelName)
                          ?.lcd_modes.find((m) => m.name === lcdModeName)?.frontend_name
                    : undefined
                result.push({
                    id: `lcd::${device.uid}::${channelName}`,
                    type: 'lcdChannel',
                    position: {
                        x: (i % FANS_PER_ROW) * COL_GAP,
                        y: currentY + Math.floor(i / FANS_PER_ROW) * ROW_GAP,
                    },
                    data: {
                        deviceUID: device.uid,
                        channelName,
                        channelLabel,
                        channelColor,
                        deviceLabel: deviceName,
                        currentMode: lcdFrontendName,
                    } satisfies LcdChannelNodeData,
                })
            }
            if (lcdChannels.length > 0) {
                const hasMore = lightingChannels.length > 0
                const lastRowGap = hasMore ? INTER_TYPE_GAP : ROW_GAP
                currentY +=
                    (Math.ceil(lcdChannels.length / FANS_PER_ROW) - 1) * ROW_GAP + lastRowGap
            }

            // Lighting nodes
            for (let i = 0; i < lightingChannels.length; i++) {
                const channelName = lightingChannels[i]
                const channelLabel =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.name ?? channelName
                const channelColor =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.color ?? '#568af2'
                const channelSetting = daemonSettings?.settings.get(channelName)
                const lightingModeName = channelSetting?.lighting?.mode
                const lightingFrontendName = lightingModeName
                    ? device
                          .info!.channels.get(channelName)
                          ?.lighting_modes.find((m) => m.name === lightingModeName)?.frontend_name
                    : undefined
                result.push({
                    id: `lighting::${device.uid}::${channelName}`,
                    type: 'lightingChannel',
                    position: {
                        x: (i % FANS_PER_ROW) * COL_GAP,
                        y: currentY + Math.floor(i / FANS_PER_ROW) * ROW_GAP,
                    },
                    data: {
                        deviceUID: device.uid,
                        channelName,
                        channelLabel,
                        channelColor,
                        deviceLabel: deviceName,
                        currentMode: lightingFrontendName,
                    } satisfies LightingChannelNodeData,
                })
            }
            if (lightingChannels.length > 0) {
                currentY += Math.ceil(lightingChannels.length / FANS_PER_ROW) * ROW_GAP
            }

            currentY += GROUP_GAP
        }

        return result
    })

    return {
        nodes,
        availableFans,
    }
}
