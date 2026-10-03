// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { onMounted, onUnmounted, ref } from 'vue'
import { mdiAlertOutline } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useToast } from '@/shell/toast'
import { useConfirm } from '@/shell/confirm'
import { useI18n } from 'vue-i18n'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import { validatePluginFetchPath, buildSafeOptions } from '@/composables/pluginFetchValidation.ts'
import { THEME_CSS_VAR_NAMES } from '@/shell/themes.ts'

export type PluginIframeMode = 'modal' | 'full_page'

// Note: the former `closeCallback` parameter was removed when the plugin UI moved from a modal
// to a full-page view. The `close()` postMessage and its handler are now no-ops kept for
// backward compatibility; there is no longer anything to close or a callback to invoke.
export function usePluginIframe(pluginId: string, mode: PluginIframeMode) {
    const deviceStore = useDeviceStore()
    const settingsStore = useSettingsStore()
    const toast = useToast()
    const confirm = useConfirm()
    const { t } = useI18n()
    const iframeRef = ref<HTMLIFrameElement | null>(null)
    const nullOriginTarget: string = '*'

    const doPluginFetch = (safePath: string, safeOptions: RequestInit, requestId: string): void => {
        const safePluginId = encodeURIComponent(pluginId)
        const url = new URL(window.location.origin)
        url.pathname = `/plugins/${safePluginId}/data${safePath.split(/[?#]/, 1)[0]}`
        const queryMatch = safePath.match(/\?([^#]*)/)
        if (queryMatch) {
            url.search = queryMatch[1]
        }
        const expectedPrefix = `${window.location.origin}/plugins/${safePluginId}/data`
        const finalUrl = url.toString()
        if (
            finalUrl !== expectedPrefix &&
            !finalUrl.startsWith(`${expectedPrefix}/`) &&
            !finalUrl.startsWith(`${expectedPrefix}?`)
        ) {
            return
        }
        fetch(finalUrl, safeOptions)
            .then((r) => (r.ok ? r.json() : null))
            .catch(() => null)
            .then((body) => {
                iframeRef.value?.contentWindow?.postMessage(
                    { type: 'pluginFetchResponse', requestId, body },
                    nullOriginTarget,
                )
            })
    }

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

    const handleRestart = (): void => {
        confirm.require({
            message: t('layout.topbar.restartConfirmMessage'),
            header: t('layout.topbar.restartConfirmHeader'),
            icon: mdiAlertOutline,
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
                // Always read the resolved CSS variable values from the parent document so
                // the iframe gets the correct colours for any theme (not just Custom).
                // Sent from the theme's own variable list rather than a copy of it here, so
                // a token added to a theme reaches plugin UIs instead of silently resolving
                // to nothing in an iframe that asked for it.
                const rootStyle = getComputedStyle(document.documentElement)
                const customStyle = Object.fromEntries(
                    THEME_CSS_VAR_NAMES.map((name) => [
                        name,
                        rootStyle.getPropertyValue(name).trim(),
                    ]),
                )
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
                // No-op: plugins previously used close() to dismiss a modal, but the plugin UI
                // is now full-page. Kept for backward compatibility with existing plugin code.
                break
            case 'restart':
                handleRestart()
                break
            case 'restartPlugin': {
                const response = await deviceStore.daemonClient.restartPlugin(pluginId)
                postToIframe('pluginRestarted', response === undefined)
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
            case 'pluginFetch': {
                const { requestId, path, options } = event.data
                if (typeof requestId !== 'string' || typeof path !== 'string') break
                const safePath = validatePluginFetchPath(path)
                if (safePath == null) break
                const safeOptions = buildSafeOptions(options)
                doPluginFetch(safePath, safeOptions, requestId)
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
