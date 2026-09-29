// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { onBeforeUnmount, onMounted, shallowRef, triggerRef, type ShallowRef } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { defaultStatsResponse, type ChannelStats, type StatsResponseDTO } from '@/models/Stats.ts'
import {
    foldStatusIntoStats,
    lifetimeStatsOf,
    lifetimeToDisplay,
    type LineKey,
} from '@/components/chartStats.ts'

export interface LifetimeStats {
    stats: ShallowRef<StatsResponseDTO>
    // One line's stats in display units, or null when nothing has been observed.
    displayOf: (line: LineKey) => ChannelStats | null
    refresh: () => Promise<void>
    reset: () => Promise<void>
}

// The daemon's min/max/avg since it started (or since the last reset), kept current between
// fetches by folding each new status in the same way the daemon does. Call from a component's
// setup: the status subscription and the visibility listener end with the component.
export function useLifetimeStats(): LifetimeStats {
    const deviceStore = useDeviceStore()
    const settingsStore = useSettingsStore()
    const stats = shallowRef<StatsResponseDTO>(defaultStatsResponse())

    const displayOf = (line: LineKey): ChannelStats | null =>
        lifetimeToDisplay(
            lifetimeStatsOf(stats.value, line),
            line.dataType,
            settingsStore.frequencyPrecision,
        )

    const refresh = async (): Promise<void> => {
        stats.value = await deviceStore.daemonClient.getStats()
    }
    const reset = async (): Promise<void> => {
        stats.value = await deviceStore.daemonClient.resetStats()
    }
    const onVisibilityChange = (): void => {
        // Folding stops while the tab is hidden, so catch up from the daemon.
        if (document.visibilityState === 'visible') refresh()
    }

    const unsubscribe = deviceStore.$onAction(({ name, after }) => {
        if (name !== 'updateStatus') return
        after((onlyRecentStatus: boolean) => {
            if (!onlyRecentStatus) {
                // A full history reload means statuses were missed; the daemon has them all.
                refresh()
                return
            }
            for (const device of deviceStore.allDevices()) {
                if (device.status == null) continue
                foldStatusIntoStats(stats.value, device.uid, device.status)
            }
            triggerRef(stats)
        })
    })

    onMounted(() => {
        document.addEventListener('visibilitychange', onVisibilityChange)
        refresh()
    })
    onBeforeUnmount(() => {
        unsubscribe()
        document.removeEventListener('visibilitychange', onVisibilityChange)
    })

    return { stats, displayOf, refresh, reset }
}
