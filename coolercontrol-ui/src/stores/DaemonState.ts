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
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'

export enum DaemonStatus {
    OK = 'Ok',
    WARN = 'Has Warnings',
    ERROR = 'Has Errors',
}

export const useDaemonState = defineStore('daemonState', () => {
    const toast = useToast()
    const { t } = useI18n()

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
            await setStatus(DaemonStatus.ERROR)
        } else if (warnings.value > 0) {
            await setStatus(DaemonStatus.WARN)
        } else {
            await setStatus(DaemonStatus.OK)
        }
    }

    async function setStatus(newStatus: DaemonStatus) {
        if (status.value === newStatus) return
        if (newStatus === DaemonStatus.ERROR) {
            toast.add({
                severity: 'error',
                summary: t('views.daemon.daemonErrors'),
                detail: t('views.daemon.daemonErrorsDetail'),
                life: 4000,
            })
        }
        status.value = newStatus
    }

    async function setConnected(isConnected: boolean): Promise<void> {
        if (connected.value === isConnected) return
        if (connected.value) {
            // disconnected
            preDisconnectedStatus.value = status.value
            toast.add({
                severity: 'error',
                summary: t('views.daemon.daemonDisconnected'),
                detail: t('views.daemon.daemonDisconnectedDetail'),
                life: 4000,
            })
            status.value = DaemonStatus.ERROR
        } else {
            // re-connected
            status.value = preDisconnectedStatus.value
            toast.add({
                severity: 'success',
                summary: t('views.daemon.connectionRestored'),
                detail: t('views.daemon.connectionRestoredMessage'),
                life: 4000,
            })
            const deviceStore = useDeviceStore()
            // re-load the logs in case the daemon has restarted
            await deviceStore.loadLogs()
            // re-check if the session is valid, in case the daemon has restarted
            await deviceStore.login()
        }
        connected.value = isConnected
    }

    async function acknowledgeLogIssues(): Promise<void> {
        if (!connected.value) return
        status.value = DaemonStatus.OK
        const deviceStore = useDeviceStore()
        await deviceStore.acknowledgeIssues()
        if (deviceStore.isQtApp()) {
            // @ts-ignore
            const ipc = window.ipc
            await ipc.acknowledgeDaemonIssues()
        }
    }

    console.debug(`Daemon State Store created`)
    return {
        init,
        setStatus,
        setConnected,
        acknowledgeLogIssues,
        systemName,
        warnings,
        errors,
        status,
        connected,
    }
})
