<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2025  Guy Boldon and contributors
  -
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  -
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  -
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'

import { useToast } from 'primevue/usetoast'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'
import { inject, onMounted, onUnmounted, ref, Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import { ThemeMode } from '@/models/UISettings.ts'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()
const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const closeDialog = () => {
    dialogRef.value.close()
}
const pluginId = dialogRef.value.data.pluginId ?? 'Unknown'
const iframeRef = ref<HTMLIFrameElement | null>(null)
const nullOriginTarget: string = '*'
const pluginUrl = (): string => {
    return `${deviceStore.daemonClient.daemonURL}plugins/${pluginId}/ui/index.html`
}

const mainStyleLinkEl = (): string | undefined => {
    for (const styleSheet of Array.from(document.styleSheets)) {
        // NOTE: the dev server will not serve the compiled stylesheet:
        // Also, complex elements are not cloneable.
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
const onIframeLoad = (): void => {}
const handleIframeMessage = async (event: MessageEvent): Promise<void> => {
    // We are sandboxing the same-origin iframe, and it will have a 'null' origin because of this.
    // We still need to verify to prevent malicious intent.
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
            iframeRef.value?.contentWindow?.postMessage(
                {
                    type: 'style',
                    body: mainStyleLinkEl(),
                },
                nullOriginTarget,
            )
            // We need to send the custom color scheme to the plugin (overrides)
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
            iframeRef.value?.contentWindow?.postMessage(
                {
                    type: 'customStyle',
                    body: customStyle,
                },
                nullOriginTarget,
            )
            break
        case 'loadConfig':
            const pluginConfig = await deviceStore.daemonClient.loadPluginConfig(pluginId)
            if (pluginConfig instanceof ErrorResponse) {
                console.error('Failed to load plugin config:', pluginConfig.error)
                break
            }
            iframeRef.value?.contentWindow?.postMessage(
                {
                    type: 'config',
                    body: pluginConfig,
                },
                nullOriginTarget,
            )
            break
        case 'saveConfig':
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
            iframeRef.value?.contentWindow?.postMessage(
                {
                    type: 'configSaved',
                    body: newPluginConfig,
                },
                nullOriginTarget,
            )
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('layout.settings.plugins.settingsSaved'),
                life: 3000,
            })
            break
        case 'close':
            closeDialog()
            break
        case 'modes':
            const modes = settingsStore.modes.map((mode) => {
                return { name: mode.name, uid: mode.uid }
            })
            iframeRef.value?.contentWindow?.postMessage(
                {
                    type: 'modes',
                    body: modes,
                },
                nullOriginTarget,
            )
            break
    }
}

onMounted(() => {
    window.addEventListener('message', handleIframeMessage)
})
onUnmounted(() => {
    window.removeEventListener('message', handleIframeMessage)
})
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <iframe
            ref="iframeRef"
            :name="`iframe-${pluginId}`"
            :src="pluginUrl()"
            class="w-full h-full"
            @load="onIframeLoad"
            sandbox="allow-scripts"
        >
            This browser does not support iframes.
        </iframe>
    </div>
</template>

<style scoped lang="scss"></style>
