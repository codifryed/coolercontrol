// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Plugin lifecycle actions with toasts, shared by the plugin page and the
// plugins panel (PrimeVue toast service stays out of the shell).

import { ref } from 'vue'
import { useToast } from '@/shell/toast'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import { PluginStatusDto } from '@/models/Plugins.ts'

// One map for every view, so a plugin stopped from its page is not still shown
// as running in the panel.
const statuses = ref<Map<string, PluginStatusDto>>(new Map())

const TOAST_LIFE_MS = 3000
// Long enough to read that a device plugin needs a daemon restart.
const TOGGLE_SUCCESS_LIFE_MS = 4000

export function usePluginActions() {
    const toast = useToast()
    const { t } = useI18n()
    const deviceStore = useDeviceStore()

    const refreshStatus = async (pluginId: string): Promise<void> => {
        const statusDto = await deviceStore.daemonClient.getPluginStatus(pluginId)
        statuses.value = new Map(statuses.value).set(pluginId, statusDto)
    }

    const refreshStatuses = async (): Promise<void> => {
        for (const plugin of deviceStore.plugins) {
            if (plugin.disabled) continue
            await refreshStatus(plugin.id)
        }
    }

    const runAction = async (
        action: (pluginId: string) => Promise<undefined | ErrorResponse>,
        pluginId: string,
        successKey: string,
        failureKey: string,
        successLife: number = TOAST_LIFE_MS,
    ): Promise<void> => {
        const response = await action(pluginId)
        if (response instanceof ErrorResponse) {
            toast.add({
                severity: 'error',
                summary: t(failureKey),
                detail: response.error,
                life: TOAST_LIFE_MS,
            })
        } else {
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t(successKey),
                life: successLife,
            })
        }
        // A start or restart re-reads the plugin's manifest.
        await deviceStore.loadAllPlugins()
        await refreshStatus(pluginId)
    }

    const startPlugin = (pluginId: string): Promise<void> =>
        runAction(
            (id) => deviceStore.daemonClient.startPlugin(id),
            pluginId,
            'layout.plugins.started',
            'layout.plugins.startFailed',
        )
    const stopPlugin = (pluginId: string): Promise<void> =>
        runAction(
            (id) => deviceStore.daemonClient.stopPlugin(id),
            pluginId,
            'layout.plugins.stopped',
            'layout.plugins.stopFailed',
        )
    const restartPlugin = (pluginId: string): Promise<void> =>
        runAction(
            (id) => deviceStore.daemonClient.restartPlugin(id),
            pluginId,
            'layout.plugins.restarted',
            'layout.plugins.restartFailed',
        )

    // A device plugin's manifest is only checked: its devices load when the daemon starts.
    const reloadPlugin = (pluginId: string, isDevicePlugin: boolean): Promise<void> =>
        runAction(
            (id) => deviceStore.daemonClient.reloadPlugin(id),
            pluginId,
            isDevicePlugin
                ? 'layout.plugins.manifestValidRestart'
                : 'layout.plugins.manifestReloaded',
            'layout.plugins.reloadFailed',
        )

    // A device plugin is only loaded or dropped when the daemon restarts.
    const enablePlugin = (pluginId: string, isDevicePlugin: boolean): Promise<void> =>
        runAction(
            (id) => deviceStore.daemonClient.enablePlugin(id),
            pluginId,
            isDevicePlugin ? 'layout.plugins.pluginEnabledRestart' : 'layout.plugins.pluginEnabled',
            'layout.plugins.enableFailed',
            TOGGLE_SUCCESS_LIFE_MS,
        )
    const disablePlugin = (pluginId: string, isDevicePlugin: boolean): Promise<void> =>
        runAction(
            (id) => deviceStore.daemonClient.disablePlugin(id),
            pluginId,
            isDevicePlugin
                ? 'layout.plugins.pluginDisabledRestart'
                : 'layout.plugins.pluginDisabled',
            'layout.plugins.disableFailed',
            TOGGLE_SUCCESS_LIFE_MS,
        )

    return {
        statuses,
        refreshStatus,
        refreshStatuses,
        startPlugin,
        stopPlugin,
        restartPlugin,
        reloadPlugin,
        enablePlugin,
        disablePlugin,
    }
}
