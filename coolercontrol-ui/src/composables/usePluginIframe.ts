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

import { onMounted, onUnmounted, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useToast } from 'primevue/usetoast'
import { useConfirm } from 'primevue/useconfirm'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import { ThemeMode } from '@/models/UISettings.ts'

export type PluginIframeMode = 'modal' | 'full_page'

export function usePluginIframe(
    pluginId: string,
    mode: PluginIframeMode,
    closeCallback?: () => void,
) {
    const deviceStore = useDeviceStore()
    const settingsStore = useSettingsStore()
    const toast = useToast()
    const confirm = useConfirm()
    const { t } = useI18n()
    const router = useRouter()
    const iframeRef = ref<HTMLIFrameElement | null>(null)
    const nullOriginTarget: string = '*'

    const pluginUrl = (entryPoint: string = 'index.html'): string => {
        return `${deviceStore.daemonClient.daemonURL}plugins/${pluginId}/ui/${entryPoint}`
    }

    const mainStyleLinkEl = (): string | undefined => {
        for (const styleSheet of Array.from(document.styleSheets)) {
            if (
                styleSheet.href &&
                styleSheet.href.includes('style') &&
                styleSheet.href.endsWith('.css')
            ) {
                return styleSheet.href
            }
        }
        console.warn('No styles found to send to plugin iframe. Are you running a dev server?')
        return undefined
    }

    const postToIframe = (type: string, body: unknown): void => {
        iframeRef.value?.contentWindow?.postMessage({ type, body }, nullOriginTarget)
    }

    const handleClose = (): void => {
        if (mode === 'modal' && closeCallback) {
            closeCallback()
        } else {
            router.back()
        }
    }

    const handleRestart = (): void => {
        handleClose()
        confirm.require({
            message: t('layout.topbar.restartConfirmMessage'),
            header: t('layout.topbar.restartConfirmHeader'),
            icon: 'pi pi-exclamation-triangle',
            defaultFocus: 'accept',
            accept: async () => {
                const successful = await deviceStore.daemonClient.shutdownDaemon()
                if (successful) {
                    toast.add({
                        severity: 'success',
                        summary: t('common.success'),
                        detail: t('layout.topbar.shutdownSuccess'),
                        life: 6000,
                    })
                    await deviceStore.waitAndReload()
                } else {
                    toast.add({
                        severity: 'error',
                        summary: t('common.error'),
                        detail: t('layout.topbar.shutdownError'),
                        life: 4000,
                    })
                }
            },
        })
    }

    const handleIframeMessage = async (event: MessageEvent): Promise<void> => {
        if (
            !event.isTrusted ||
            event.origin !== 'null' ||
            event.source == null ||
            event.source !== iframeRef.value?.contentWindow ||
            event.data == null ||
            event.data.type == null
        ) {
            console.debug('Received Invalid Message')
            return
        }
        switch (event.data.type) {
            case 'style':
                postToIframe('style', mainStyleLinkEl())
                break
            case 'customStyle': {
                const customStyle =
                    settingsStore.themeMode === ThemeMode.CUSTOM
                        ? {
                              '--colors-accent': settingsStore.customTheme.accent,
                              '--colors-bg-one': settingsStore.customTheme.bgOne,
                              '--colors-bg-two': settingsStore.customTheme.bgTwo,
                              '--colors-border-one': settingsStore.customTheme.borderOne,
                              '--colors-text-color': settingsStore.customTheme.textColor,
                              '--colors-text-color-secondary':
                                  settingsStore.customTheme.textColorSecondary,
                          }
                        : undefined
                postToIframe('customStyle', customStyle)
                break
            }
            case 'loadConfig': {
                const pluginConfig = await deviceStore.daemonClient.loadPluginConfig(pluginId)
                if (pluginConfig instanceof ErrorResponse) {
                    console.error('Failed to load plugin config:', pluginConfig.error)
                    break
                }
                postToIframe('config', pluginConfig)
                break
            }
            case 'saveConfig': {
                if (event.data.body == null) {
                    console.error('Failed to save plugin config: No config provided')
                    break
                }
                let newPluginConfig = event.data.body
                const response = await deviceStore.daemonClient.savePluginConfig(
                    pluginId,
                    newPluginConfig,
                )
                if (response instanceof ErrorResponse) {
                    console.error('Failed to save plugin config:', response.error)
                    newPluginConfig = null
                    toast.add({
                        severity: 'error',
                        summary: t('common.error'),
                        detail: t('layout.settings.plugins.settingsNotSaved'),
                        life: 3000,
                    })
                }
                postToIframe('configSaved', newPluginConfig)
                toast.add({
                    severity: 'success',
                    summary: t('common.success'),
                    detail: t('layout.settings.plugins.settingsSaved'),
                    life: 3000,
                })
                break
            }
            case 'close':
                handleClose()
                break
            case 'restart':
                handleRestart()
                break
            case 'restartPlugin': {
                const success = await deviceStore.daemonClient.restartPlugin(pluginId)
                postToIframe('pluginRestarted', success)
                break
            }
            case 'context':
                postToIframe('context', { mode })
                break
            case 'modes': {
                const modes = settingsStore.modes.map((m) => ({
                    name: m.name,
                    uid: m.uid,
                }))
                postToIframe('modes', modes)
                break
            }
            case 'alerts': {
                const alerts = settingsStore.alerts.map((alert) => ({
                    name: alert.name,
                    uid: alert.uid,
                }))
                postToIframe('alerts', alerts)
                break
            }
            case 'profiles': {
                const profiles = settingsStore.profiles.map((profile) => ({
                    name: profile.name,
                    uid: profile.uid,
                    p_type: profile.p_type,
                }))
                postToIframe('profiles', profiles)
                break
            }
            case 'functions': {
                const functions = settingsStore.functions.map((fn) => ({
                    name: fn.name,
                    uid: fn.uid,
                    p_type: fn.f_type,
                }))
                postToIframe('functions', functions)
                break
            }
            case 'devices': {
                const devices = []
                for (const device of deviceStore.allDevices()) {
                    devices.push({
                        name: device.name,
                        uid: device.uid,
                        type: device.type,
                        info: device.info,
                    })
                }
                postToIframe('devices', devices)
                break
            }
            case 'status': {
                const status = new Map()
                for (const [
                    deviceUID,
                    deviceChannelStatus,
                ] of deviceStore.currentDeviceStatus.entries()) {
                    status.set(deviceUID, deviceChannelStatus)
                }
                postToIframe('status', status)
                break
            }
        }
    }

    onMounted(() => {
        window.addEventListener('message', handleIframeMessage)
    })
    onUnmounted(() => {
        window.removeEventListener('message', handleIframeMessage)
    })

    return { iframeRef, pluginUrl }
}
