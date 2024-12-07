/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
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

import { defineStore } from 'pinia'
import { ref, Ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'

export enum DaemonStatus {
    OK = 'Ok',
    WARN = 'Has Warnings',
    ERROR = 'Has Errors',
}

export const useDaemonState = defineStore('daemonState', () => {
    // Reactive properties ------------------------------------------------
    const systemName: Ref<string> = ref('Localhost')
    const warnings: Ref<number> = ref(0)
    const errors: Ref<number> = ref(0)
    const status: Ref<DaemonStatus> = ref(DaemonStatus.ERROR)
    const connected: Ref<boolean> = ref(false)
    const preDisconnectedStatus: Ref<DaemonStatus> = ref(DaemonStatus.ERROR)

    async function init(): Promise<void> {
        const deviceStore = useDeviceStore()
        const healthCheck = await deviceStore.health()
        systemName.value = healthCheck.system.name
        warnings.value = healthCheck.details.warnings
        errors.value = healthCheck.details.errors
        connected.value = true
        if (errors.value > 0) {
            status.value = DaemonStatus.ERROR
        } else if (warnings.value > 0) {
            status.value = DaemonStatus.WARN
        } else {
            status.value = DaemonStatus.OK
        }
    }

    function setConnected(isConnected: boolean): void {
        if (connected.value === isConnected) return
        if (connected.value) {
            // disconnected
            preDisconnectedStatus.value = status.value
            status.value = DaemonStatus.ERROR
        } else {
            // re-connected
            status.value = preDisconnectedStatus.value
        }
        connected.value = isConnected
    }

    async function acknowledgeLogIssues(): Promise<void> {
        if (!connected.value) return
        status.value = DaemonStatus.OK
    }

    console.debug(`Daemon State Store created`)
    return {
        init,
        setConnected,
        acknowledgeLogIssues,
        systemName,
        warnings,
        errors,
        status,
    }
})
