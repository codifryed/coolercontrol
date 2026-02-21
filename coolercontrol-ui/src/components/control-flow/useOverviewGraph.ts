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

const deviceStore = useDeviceStore()
const FANS_PER_ROW = 3
const COL_GAP = deviceStore.getREMSize(20)
const ROW_GAP = deviceStore.getREMSize(10)
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

            // Get ordered channels
            const channels: string[] = []
            for (const [channelName, channelInfo] of device.info!.channels.entries()) {
                if (channelInfo.speed_options?.fixed_enabled) {
                    channels.push(channelName)
                }
            }

            if (channels.length === 0) continue

            // Sort channels by menuOrder children
            const menuItem = settingsStore.menuOrder.find((m) => m.id === device.uid)
            if (menuItem?.children?.length) {
                const getChildIndex = (channelName: string) => {
                    const idx = menuItem.children.indexOf(`${device.uid}_${channelName}`)
                    return idx >= 0 ? idx : Number.MAX_SAFE_INTEGER
                }
                channels.sort((a, b) => getChildIndex(a) - getChildIndex(b))
            }

            // Add device label node
            const labelNodeId = `device-label::${device.uid}`
            result.push({
                id: labelNodeId,
                type: 'deviceLabel',
                position: { x: 0, y: currentY },
                data: {
                    deviceName,
                    deviceColor,
                    channelCount: channels.length,
                } satisfies DeviceLabelNodeData,
            })
            currentY += DEVICE_LABEL_HEIGHT

            // Add fan nodes in rows
            for (let i = 0; i < channels.length; i++) {
                const channelName = channels[i]
                const col = i % FANS_PER_ROW
                const row = Math.floor(i / FANS_PER_ROW)

                const channelLabel =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.name ?? channelName
                const channelColor =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.color ?? '#568af2'

                const channelSetting = daemonSettings?.settings.get(channelName)
                const isManual = channelSetting?.speed_fixed != null
                const profileUID = channelSetting?.profile_uid

                const fanNodeId = `fan::${device.uid}::${channelName}`
                result.push({
                    id: fanNodeId,
                    type: 'fanChannel',
                    position: {
                        x: col * COL_GAP,
                        y: currentY + row * ROW_GAP,
                    },
                    data: {
                        deviceUID: device.uid,
                        channelName,
                        channelLabel,
                        channelColor,
                        deviceLabel: deviceName,
                        isManual,
                        manualDuty: channelSetting?.speed_fixed,
                        profileUID,
                    } satisfies FanNodeData,
                })
            }

            const rowCount = Math.ceil(channels.length / FANS_PER_ROW)
            currentY += rowCount * ROW_GAP + GROUP_GAP
        }

        return result
    })

    return {
        nodes,
        availableFans,
    }
}
