// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { defineStore } from 'pinia'
import { computed, ref, Ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useToast } from '@/shell/toast'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import defaultHealthCheck, { type HealthCheck } from '@/models/HealthCheck.ts'
import {
    connectionLostThresholdMs,
    formatDisconnectedFor,
    isConnectionLost,
} from '@/shell/connectionWatchdog.ts'
import { debugForcedBy, isDebugLogging } from '@/shell/debugLogging.ts'

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
    // Kept here so views render it without their own blocking fetch. Falls back to
    // defaultHealthCheck's empty values until the first refresh lands.
    const healthCheck: Ref<HealthCheck> = ref(defaultHealthCheck())
    // The level only changes on a daemon restart, which reloads the UI, so the
    // health check fetched at boot stays accurate.
    const debugLogging = computed(() => isDebugLogging(healthCheck.value.details))
    const debugLoggingForcedBy = computed(() => debugForcedBy(healthCheck.value.details))
    // Connection liveness is measured from the last status tick rather than from
    // the SSE error edge: a stream that stays open but stops delivering leaves
    // `connected` true while the readings go stale.
    const lastStatusAt: Ref<number> = ref(Date.now())
    const connectionLost: Ref<boolean> = ref(false)
    const disconnectedFor: Ref<string> = ref('')
    let watchdog: ReturnType<typeof setInterval> | undefined

    async function init(): Promise<void> {
        await refreshHealth()
        connected.value = true
        startWatchdog()
        if (errors.value > 0) {
            await setStatus(DaemonStatus.ERROR)
        } else if (warnings.value > 0) {
            await setStatus(DaemonStatus.WARN)
        } else {
            await setStatus(DaemonStatus.OK)
        }
    }

    // daemonClient.health() falls back to defaults instead of throwing, so callers
    // may fire this without awaiting it.
    async function refreshHealth(): Promise<void> {
        const deviceStore = useDeviceStore()
        healthCheck.value = await deviceStore.health()
        systemName.value = healthCheck.value.system.name
        warnings.value = healthCheck.value.details.warnings
        errors.value = healthCheck.value.details.errors
    }

    function noteStatusReceived(): void {
        lastStatusAt.value = Date.now()
        // Cleared here rather than left to the next tick so the overlay never
        // outlives the frame that ended the outage.
        connectionLost.value = false
    }

    // Started from init() so importing the store never spawns a timer, and so the
    // boot path stays with the connection-error modal that already owns it.
    function startWatchdog(): void {
        if (watchdog != null) return
        // Boot can outrun the grace period on a slow system, and the clock should
        // measure the stream, not how long startup took.
        noteStatusReceived()
        const settingsStore = useSettingsStore()
        watchdog = setInterval(() => {
            // Read per tick, not captured: the poll rate is a live setting and the
            // threshold has to follow it without a restart.
            const threshold = connectionLostThresholdMs(settingsStore.ccSettings.poll_rate)
            const elapsed = Date.now() - lastStatusAt.value
            connectionLost.value = isConnectionLost(elapsed, threshold)
            if (connectionLost.value) disconnectedFor.value = formatDisconnectedFor(elapsed)
        }, 1_000)
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
        // If the connection state hasn't changed, do nothing
        if (connected.value === isConnected) return
        connected.value = isConnected
        if (isConnected) {
            // re-connected
            status.value = preDisconnectedStatus.value
            // force-reloading the UI results in a better UX, especially when the daemon is restarted
            const deviceStore = useDeviceStore()
            if (deviceStore.isQtApp()) {
                const settingsStore = useSettingsStore()
                await deviceStore.waitAndReload(settingsStore.desktopStartupDelay)
            } else {
                deviceStore.reloadUI(true)
            }
            // Previous behavior:
            // toast.add({
            //     severity: 'success',
            //     summary: t('views.daemon.connectionRestored'),
            //     detail: t('views.daemon.connectionRestoredMessage'),
            //     life: 4000,
            // })
            // re-load the logs in case the daemon has restarted
            // await deviceStore.loadLogs()
            // re-check if the session is valid, in case the daemon has restarted
            // await deviceStore.login()
            return
        }
        // else disconnected. No toast: a drop that resolves within the watchdog's
        // grace period is invisible on purpose, and a lasting one gets the overlay.
        preDisconnectedStatus.value = status.value
        status.value = DaemonStatus.ERROR
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
        refreshHealth,
        setStatus,
        setConnected,
        noteStatusReceived,
        acknowledgeLogIssues,
        healthCheck,
        debugLogging,
        debugLoggingForcedBy,
        systemName,
        warnings,
        errors,
        status,
        connected,
        connectionLost,
        disconnectedFor,
    }
})
