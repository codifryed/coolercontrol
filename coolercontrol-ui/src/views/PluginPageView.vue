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
import {
    mdiInformationOutline,
    mdiLinkVariant,
    mdiPlay,
    mdiPowerPlugOutline,
    mdiRestart,
    mdiStop,
} from '@mdi/js'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { usePluginIframe } from '@/composables/usePluginIframe.ts'
import { useDialog } from 'primevue/usedialog'
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'
import {
    getPluginStatusDisplayName,
    PluginDto,
    PluginStatus,
    ServiceType,
} from '@/models/Plugins.ts'
import pluginMetadataModal from '@/layout/PluginUi.vue'
import Button from 'primevue/button'
import Tag from 'primevue/tag'
import ToggleSwitch from 'primevue/toggleswitch'

const STATUS_POLL_INTERVAL_MS = 30_000

const props = defineProps<{ pluginId: string }>()

const deviceStore = useDeviceStore()
const dialog = useDialog()
const toast = useToast()
const { t } = useI18n()

const plugin = ref<PluginDto | null>(null)
const hasUi = ref(false)
const pluginStatus = ref<PluginStatus>(PluginStatus.Unmanaged)
const pluginStatusReason = ref<string | undefined>(undefined)
const loading = ref(true)
let statusPollTimer: ReturnType<typeof setInterval> | undefined

const pluginIframe = usePluginIframe(props.pluginId, 'full_page')

const isIntegration = computed(() => plugin.value?.service_type === ServiceType.Integration)
const isDisabled = computed(() => plugin.value?.disabled ?? false)
const isManaged = computed(
    () => isIntegration.value && !isDisabled.value && pluginStatus.value !== PluginStatus.Unmanaged,
)

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
    const pluginsDto = await deviceStore.daemonClient.loadPlugins()
    plugin.value = pluginsDto.plugins.find((p) => p.id === props.pluginId) ?? null
    const uiInfo = await deviceStore.daemonClient.hasPluginUi(props.pluginId)
    hasUi.value = uiInfo.has_ui
    await refreshStatus()
    loading.value = false
    statusPollTimer = setInterval(refreshStatus, STATUS_POLL_INTERVAL_MS)
}

const refreshStatus = async (): Promise<void> => {
    const statusDto = await deviceStore.daemonClient.getPluginStatus(props.pluginId)
    pluginStatus.value = statusDto.status as PluginStatus
    pluginStatusReason.value = statusDto.reason
}

const startPlugin = async (): Promise<void> => {
    const success = await deviceStore.daemonClient.startPlugin(props.pluginId)
    if (success) {
        toast.add({
            severity: 'success',
            summary: t('common.success'),
            detail: t('layout.plugins.started'),
            life: 3000,
        })
    } else {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('layout.plugins.startFailed'),
            life: 3000,
        })
    }
    await refreshStatus()
}

const stopPlugin = async (): Promise<void> => {
    const success = await deviceStore.daemonClient.stopPlugin(props.pluginId)
    if (success) {
        toast.add({
            severity: 'success',
            summary: t('common.success'),
            detail: t('layout.plugins.stopped'),
            life: 3000,
        })
    } else {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('layout.plugins.stopFailed'),
            life: 3000,
        })
    }
    await refreshStatus()
}

const restartPlugin = async (): Promise<void> => {
    const success = await deviceStore.daemonClient.restartPlugin(props.pluginId)
    if (success) {
        toast.add({
            severity: 'success',
            summary: t('common.success'),
            detail: t('layout.plugins.restarted'),
            life: 3000,
        })
    } else {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('layout.plugins.restartFailed'),
            life: 3000,
        })
    }
    await refreshStatus()
}

const togglePlugin = async (): Promise<void> => {
    if (isDisabled.value) {
        const success = await deviceStore.daemonClient.enablePlugin(props.pluginId)
        if (success) {
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
                summary: t('common.error'),
                detail: t('layout.plugins.enableFailed'),
                life: 3000,
            })
        }
    } else {
        const success = await deviceStore.daemonClient.disablePlugin(props.pluginId)
        if (success) {
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
                summary: t('common.error'),
                detail: t('layout.plugins.disableFailed'),
                life: 3000,
            })
        }
    }
    await loadPluginData()
    await deviceStore.loadAllPlugins()
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
        },
    })
}

onMounted(loadPluginData)
onUnmounted(() => {
    if (statusPollTimer != null) {
        clearInterval(statusPollTimer)
    }
})
</script>

<template>
    <div v-if="loading" class="flex items-center justify-center w-full h-full">
        <i class="pi pi-spin pi-spinner text-4xl text-text-color-secondary" />
    </div>
    <div v-else-if="plugin == null" class="flex items-center justify-center w-full h-full">
        <span class="text-text-color-secondary text-lg">{{ t('layout.plugins.notFound') }}</span>
    </div>
    <div v-else class="flex flex-col w-full h-full">
        <!-- Header toolbar -->
        <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
            <div class="pl-4 py-2 flex items-center gap-3">
                <svg-icon
                    type="mdi"
                    :path="mdiPowerPlugOutline"
                    :size="deviceStore.getREMSize(1.75)"
                />
                <span class="text-2xl font-bold">{{ plugin.id }}</span>
                <span v-if="plugin.version" class="text-text-color-secondary text-sm">
                    v{{ plugin.version }}
                </span>
                <Tag :value="statusDisplayName" :severity="statusSeverity" class="ml-2" />
                <span v-if="pluginStatusReason" class="text-text-color-secondary text-sm italic">
                    {{ pluginStatusReason }}
                </span>
            </div>

            <div class="pr-4 flex items-center gap-2">
                <!-- Enable/disable toggle -->
                <ToggleSwitch
                    v-tooltip.bottom="
                        isDisabled ? t('layout.plugins.enable') : t('layout.plugins.disable')
                    "
                    :model-value="!isDisabled"
                    @update:model-value="togglePlugin"
                />

                <!-- Lifecycle controls (managed integration plugins only, when enabled) -->
                <template v-if="isManaged">
                    <Button
                        v-tooltip.bottom="t('layout.plugins.start')"
                        class="!p-1.5"
                        :disabled="pluginStatus === PluginStatus.Running"
                        severity="success"
                        text
                        @click="startPlugin"
                    >
                        <svg-icon type="mdi" :path="mdiPlay" :size="deviceStore.getREMSize(1.5)" />
                    </Button>
                    <Button
                        v-tooltip.bottom="t('layout.plugins.stop')"
                        class="!p-1.5"
                        :disabled="pluginStatus === PluginStatus.Stopped"
                        severity="danger"
                        text
                        @click="stopPlugin"
                    >
                        <svg-icon type="mdi" :path="mdiStop" :size="deviceStore.getREMSize(1.5)" />
                    </Button>
                    <Button
                        v-tooltip.bottom="t('layout.plugins.restart')"
                        class="!p-1.5"
                        text
                        @click="restartPlugin"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiRestart"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                    </Button>
                </template>

                <!-- Plugin info button (visible for plugins with UI) -->
                <Button
                    v-if="hasUi"
                    v-tooltip.bottom="t('layout.plugins.info')"
                    class="!bg-accent/80 hover:!bg-accent h-[2.375rem] !border !border-border-one"
                    text
                    @click="openMetadataModal"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiInformationOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                </Button>
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
                        <tr v-if="plugin.url">
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
                    </tbody>
                </table>
            </div>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
