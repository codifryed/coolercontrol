<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiContentCopy,
    mdiInformationOutline,
    mdiLinkVariant,
    mdiLoading,
    mdiPlay,
    mdiPowerPlugOutline,
    mdiRestart,
    mdiStop,
} from '@mdi/js'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { usePluginIframe } from '@/composables/usePluginIframe.ts'
import { usePluginActions } from '@/composables/usePluginActions.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import { useDialog } from '@/shell/dialog'
import { useToast } from '@/shell/toast'
import { useI18n } from 'vue-i18n'
import {
    getPluginStatusDisplayName,
    PluginDto,
    PluginStatus,
    ServiceType,
} from '@/models/Plugins.ts'
import pluginMetadataModal from '@/layout/PluginUi.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import UiSwitch from '@/shell/ui/UiSwitch.vue'
import UiTag from '@/shell/ui/UiTag.vue'

const STATUS_POLL_INTERVAL_MS = 30_000

const props = defineProps<{ pluginId: string }>()

const deviceStore = useDeviceStore()
const dialog = useDialog()
const toast = useToast()
const { t } = useI18n()
const pluginActions = usePluginActions()

// From the store, so a manifest re-read by a start or restart shows up here.
const plugin = computed(
    (): PluginDto | null => deviceStore.plugins.find((p) => p.id === props.pluginId) ?? null,
)
const hasUi = computed(() => deviceStore.pluginUiInfo.get(props.pluginId)?.has_ui ?? false)
const pluginStatus = computed(
    () => pluginActions.statuses.value.get(props.pluginId)?.status ?? PluginStatus.Unmanaged,
)
const pluginStatusReason = computed(() => pluginActions.statuses.value.get(props.pluginId)?.reason)
const loading = ref(true)
let statusPollTimer: ReturnType<typeof setInterval> | undefined

const pluginIframe = usePluginIframe(props.pluginId, 'full_page')

const isDisabled = computed(() => plugin.value?.disabled ?? false)
const isManaged = computed(() => !isDisabled.value && pluginStatus.value !== PluginStatus.Unmanaged)
const isIntegration = computed(() => plugin.value?.service_type === ServiceType.Integration)

const statusSeverity = computed((): 'success' | 'danger' | 'secondary' | 'warn' => {
    if (isDisabled.value) return 'warn'
    switch (pluginStatus.value) {
        case PluginStatus.Running:
            return 'success'
        case PluginStatus.Stopped:
            return 'danger'
        default:
            return 'secondary'
    }
})

const statusDisplayName = computed(() => {
    if (isDisabled.value) return t('models.pluginStatus.disabled')
    return getPluginStatusDisplayName(pluginStatus.value)
})

const loadPluginData = async (): Promise<void> => {
    await deviceStore.loadAllPlugins()
    await refreshStatus()
    loading.value = false
}

const refreshStatus = (): Promise<void> => pluginActions.refreshStatus(props.pluginId)
const startPlugin = (): Promise<void> => pluginActions.startPlugin(props.pluginId)
const stopPlugin = (): Promise<void> => pluginActions.stopPlugin(props.pluginId)
const restartPlugin = (): Promise<void> => pluginActions.restartPlugin(props.pluginId)

const togglePlugin = async (): Promise<void> => {
    if (isDisabled.value) {
        const response = await deviceStore.daemonClient.enablePlugin(props.pluginId)
        if (!(response instanceof ErrorResponse)) {
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: isIntegration.value
                    ? t('layout.plugins.pluginEnabled')
                    : t('layout.plugins.pluginEnabledRestart'),
                life: 4000,
            })
        } else {
            toast.add({
                severity: 'error',
                summary: t('layout.plugins.enableFailed'),
                detail: response.error,
                life: 3000,
            })
        }
    } else {
        const response = await deviceStore.daemonClient.disablePlugin(props.pluginId)
        if (!(response instanceof ErrorResponse)) {
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: isIntegration.value
                    ? t('layout.plugins.pluginDisabled')
                    : t('layout.plugins.pluginDisabledRestart'),
                life: 4000,
            })
        } else {
            toast.add({
                severity: 'error',
                summary: t('layout.plugins.disableFailed'),
                detail: response.error,
                life: 3000,
            })
        }
    }
    await loadPluginData()
}

const copyCommand = (text: string): void => {
    if (navigator.clipboard?.writeText) {
        navigator.clipboard.writeText(text).catch(() => fallbackCopy(text))
    } else {
        fallbackCopy(text)
    }
    toast.add({
        severity: 'info',
        summary: t('layout.plugins.commandCopied'),
        life: 2000,
    })
}

const fallbackCopy = (text: string): void => {
    const textarea = document.createElement('textarea')
    textarea.value = text
    textarea.style.position = 'fixed'
    textarea.style.opacity = '0'
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
}

const openMetadataModal = (): void => {
    if (plugin.value == null) return
    dialog.open(pluginMetadataModal, {
        props: {
            header: `${props.pluginId} ${t('layout.plugins.info')}`,
            position: 'center',
            modal: true,
            dismissableMask: true,
        },
        data: {
            plugin: plugin.value,
            isManaged: isManaged.value,
        },
    })
}

onMounted(async () => {
    await loadPluginData()
    statusPollTimer = setInterval(refreshStatus, STATUS_POLL_INTERVAL_MS)
})
onUnmounted(() => {
    if (statusPollTimer != null) {
        clearInterval(statusPollTimer)
    }
})
</script>

<template>
    <div v-if="loading" class="flex items-center justify-center w-full h-full">
        <svg-icon
            type="mdi"
            :path="mdiLoading"
            :size="36"
            class="animate-spin text-text-color-secondary"
        />
    </div>
    <div v-else-if="plugin == null" class="flex items-center justify-center w-full h-full">
        <span class="text-text-color-secondary text-lg">{{ t('layout.plugins.notFound') }}</span>
    </div>
    <div v-else class="flex flex-col w-full h-full">
        <!-- Header toolbar -->
        <div class="flex items-center justify-between p-4">
            <div class="flex items-center gap-3">
                <svg-icon
                    type="mdi"
                    :path="mdiPowerPlugOutline"
                    :size="deviceStore.getREMSize(1.75)"
                />
                <h1 class="text-xl font-semibold text-text-color">{{ plugin.id }}</h1>
                <span v-if="plugin.version" class="text-text-color-secondary text-sm">
                    v{{ plugin.version }}
                </span>
                <UiTag :value="statusDisplayName" :severity="statusSeverity" class="ml-2" />
                <span v-if="pluginStatusReason" class="text-text-color-secondary text-sm italic">
                    {{ pluginStatusReason }}
                </span>
            </div>

            <div class="flex items-center gap-2">
                <!-- Enable/disable toggle -->
                <UiSwitch
                    v-tooltip.top="
                        isDisabled ? t('layout.plugins.enable') : t('layout.plugins.disable')
                    "
                    :model-value="!isDisabled"
                    @update:model-value="togglePlugin"
                />

                <!-- Lifecycle controls (managed integration plugins only, when enabled) -->
                <template v-if="isManaged && isIntegration">
                    <UiButton
                        v-tooltip.top="t('layout.plugins.start')"
                        variant="ghost"
                        size="icon"
                        class="text-success"
                        :disabled="pluginStatus === PluginStatus.Running"
                        @click="startPlugin"
                    >
                        <svg-icon type="mdi" :path="mdiPlay" :size="deviceStore.getREMSize(1.5)" />
                    </UiButton>
                    <UiButton
                        v-tooltip.top="t('layout.plugins.stop')"
                        variant="ghost"
                        size="icon"
                        class="text-error"
                        :disabled="pluginStatus === PluginStatus.Stopped"
                        @click="stopPlugin"
                    >
                        <svg-icon type="mdi" :path="mdiStop" :size="deviceStore.getREMSize(1.5)" />
                    </UiButton>
                    <UiButton
                        v-tooltip.top="t('layout.plugins.restart')"
                        variant="ghost"
                        size="icon"
                        @click="restartPlugin"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiRestart"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                    </UiButton>
                </template>

                <!-- Plugin info button (visible for plugins with UI) -->
                <UiButton
                    v-if="hasUi"
                    v-tooltip.top="t('layout.plugins.info')"
                    variant="outline"
                    size="icon"
                    class="!bg-accent/80 hover:!bg-accent"
                    @click="openMetadataModal"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiInformationOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                </UiButton>
            </div>
        </div>

        <!-- Full-page iframe (only when enabled and has UI) -->
        <div v-if="hasUi && !isDisabled" class="flex-1 min-h-0">
            <iframe
                :ref="
                    (el: any) => {
                        pluginIframe.iframeRef.value = el
                    }
                "
                :name="`iframe-fullpage-${pluginId}`"
                :src="pluginIframe.pluginUrl()"
                class="w-full h-full border-0"
                sandbox="allow-scripts"
            >
                This browser does not support iframes.
            </iframe>
        </div>
        <div v-else class="flex-1 p-6 overflow-auto">
            <!-- Info/management page for disabled plugins or plugins without a UI -->
            <div class="max-w-2xl mx-auto space-y-4">
                <div v-if="plugin.description" class="text-wrap italic text-text-color-secondary">
                    {{ plugin.description }}
                </div>
                <table class="bg-bg-two rounded-lg w-full">
                    <tbody>
                        <tr class="border-b border-border-one">
                            <td class="py-3 px-4 font-semibold w-40">
                                {{ t('layout.plugins.type') }}
                            </td>
                            <td class="py-3 px-4">{{ plugin.service_type }}</td>
                        </tr>
                        <tr v-if="plugin.address" class="border-b border-border-one">
                            <td class="py-3 px-4 font-semibold">
                                {{ t('layout.plugins.address') }}
                            </td>
                            <td class="py-3 px-4">
                                <code>{{ plugin.address }}</code>
                            </td>
                        </tr>
                        <tr class="border-b border-border-one">
                            <td class="py-3 px-4 font-semibold">
                                {{ t('layout.plugins.privileges') }}
                            </td>
                            <td class="py-3 px-4" :class="{ 'font-bold': plugin.privileged }">
                                {{
                                    plugin.privileged
                                        ? t('layout.settings.plugins.privileged')
                                        : t('layout.settings.plugins.restricted')
                                }}
                            </td>
                        </tr>
                        <tr v-if="plugin.url" :class="{ 'border-b border-border-one': isManaged }">
                            <td class="py-3 px-4 font-semibold">{{ t('layout.plugins.url') }}</td>
                            <td class="py-3 px-4">
                                <a
                                    class="inline-flex items-center gap-1 underline"
                                    :href="plugin.url"
                                    target="_blank"
                                    rel="noopener noreferrer"
                                >
                                    <svg-icon
                                        type="mdi"
                                        :path="mdiLinkVariant"
                                        :size="deviceStore.getREMSize(1)"
                                    />
                                    {{ t('layout.settings.plugins.pluginUrl') }}
                                </a>
                            </td>
                        </tr>
                        <tr v-if="isManaged">
                            <td class="py-3 px-4 font-semibold align-top">
                                {{ t('layout.plugins.serviceLogs') }}
                            </td>
                            <td class="py-3 px-4 space-y-1">
                                <div class="flex items-center gap-1">
                                    <span class="text-xs text-text-color-secondary">systemd:</span>
                                    <code class="ml-1 select-all"
                                        >journalctl -f -u cc-plugin-{{ plugin.id }}</code
                                    >
                                    <svg-icon
                                        type="mdi"
                                        :path="mdiContentCopy"
                                        :size="14"
                                        class="cursor-pointer opacity-60 hover:opacity-100"
                                        @click="
                                            copyCommand(`journalctl -f -u cc-plugin-${plugin.id}`)
                                        "
                                    />
                                </div>
                                <div class="flex items-center gap-1">
                                    <span class="text-xs text-text-color-secondary">OpenRC:</span>
                                    <code class="ml-1 select-all"
                                        >grep cc-plugin-{{ plugin.id }} /var/log/messages</code
                                    >
                                    <svg-icon
                                        type="mdi"
                                        :path="mdiContentCopy"
                                        :size="14"
                                        class="cursor-pointer opacity-60 hover:opacity-100"
                                        @click="
                                            copyCommand(
                                                `grep cc-plugin-${plugin.id} /var/log/messages`,
                                            )
                                        "
                                    />
                                </div>
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
