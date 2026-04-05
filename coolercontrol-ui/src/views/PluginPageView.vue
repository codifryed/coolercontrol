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
    mdiCogs,
    mdiLinkVariant,
    mdiPlay,
    mdiPowerPlugOutline,
    mdiRestart,
    mdiStop,
} from '@mdi/js'
import { computed, onMounted, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { usePluginIframe } from '@/composables/usePluginIframe.ts'
import { useDialog } from 'primevue/usedialog'
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'
import { PluginDto, PluginStatus, ServiceType } from '@/models/Plugins.ts'
import pluginUiModal from '@/layout/PluginUi.vue'
import Button from 'primevue/button'
import Tag from 'primevue/tag'

const props = defineProps<{ pluginId: string }>()

const deviceStore = useDeviceStore()
const dialog = useDialog()
const toast = useToast()
const { t } = useI18n()

const plugin = ref<PluginDto | null>(null)
const hasSettingsUi = ref(false)
const hasFullPageUi = ref(false)
const pluginStatus = ref<PluginStatus>(PluginStatus.NotInstalled)
const pluginStatusReason = ref<string | undefined>(undefined)
const loading = ref(true)

const pluginIframe = usePluginIframe(props.pluginId, 'full_page')

const isIntegration = computed(() => plugin.value?.service_type === ServiceType.Integration)
const isManaged = computed(() => isIntegration.value && pluginStatus.value !== PluginStatus.NotInstalled)

const statusSeverity = computed((): 'success' | 'danger' | 'secondary' => {
    switch (pluginStatus.value) {
        case PluginStatus.Running:
            return 'success'
        case PluginStatus.Stopped:
            return 'danger'
        default:
            return 'secondary'
    }
})

const loadPluginData = async (): Promise<void> => {
    const pluginsDto = await deviceStore.daemonClient.loadPlugins()
    plugin.value = pluginsDto.plugins.find((p) => p.id === props.pluginId) ?? null
    const uiInfo = await deviceStore.daemonClient.hasPluginUi(props.pluginId)
    hasSettingsUi.value = uiInfo.has_ui
    hasFullPageUi.value = uiInfo.has_full_page_ui
    await refreshStatus()
    loading.value = false
}

const refreshStatus = async (): Promise<void> => {
    const statusDto = await deviceStore.daemonClient.getPluginStatus(props.pluginId)
    pluginStatus.value = statusDto.status as PluginStatus
    pluginStatusReason.value = statusDto.reason
}

const startPlugin = async (): Promise<void> => {
    const success = await deviceStore.daemonClient.startPlugin(props.pluginId)
    if (success) {
        toast.add({ severity: 'success', summary: t('common.success'), detail: t('layout.plugins.started'), life: 3000 })
    } else {
        toast.add({ severity: 'error', summary: t('common.error'), detail: t('layout.plugins.startFailed'), life: 3000 })
    }
    await refreshStatus()
}

const stopPlugin = async (): Promise<void> => {
    const success = await deviceStore.daemonClient.stopPlugin(props.pluginId)
    if (success) {
        toast.add({ severity: 'success', summary: t('common.success'), detail: t('layout.plugins.stopped'), life: 3000 })
    } else {
        toast.add({ severity: 'error', summary: t('common.error'), detail: t('layout.plugins.stopFailed'), life: 3000 })
    }
    await refreshStatus()
}

const restartPlugin = async (): Promise<void> => {
    const success = await deviceStore.daemonClient.restartPlugin(props.pluginId)
    if (success) {
        toast.add({ severity: 'success', summary: t('common.success'), detail: t('layout.plugins.restarted'), life: 3000 })
    } else {
        toast.add({ severity: 'error', summary: t('common.error'), detail: t('layout.plugins.restartFailed'), life: 3000 })
    }
    await refreshStatus()
}

const openSettingsModal = (): void => {
    dialog.open(pluginUiModal, {
        props: {
            header: `${props.pluginId} ${t('layout.topbar.settings')}`,
            position: 'center',
            modal: true,
            dismissableMask: true,
        },
        data: {
            pluginId: props.pluginId,
        },
    })
}

onMounted(loadPluginData)
</script>

<template>
    <div v-if="loading" class="flex items-center justify-center w-full h-full">
        <i class="pi pi-spin pi-spinner text-4xl text-text-color-secondary" />
    </div>
    <div v-else-if="plugin == null" class="flex items-center justify-center w-full h-full">
        <span class="text-text-color-secondary text-lg">{{ t('layout.plugins.notFound') }}</span>
    </div>
    <div v-else class="flex flex-col w-full h-full">
        <!-- Compact toolbar -->
        <div class="flex items-center gap-3 px-4 py-2 border-b border-border-one bg-bg-two shrink-0">
            <svg-icon type="mdi" :path="mdiPowerPlugOutline" :size="deviceStore.getREMSize(1.5)" />
            <span class="font-semibold text-lg">{{ plugin.id }}</span>
            <span v-if="plugin.version" class="text-text-color-secondary text-sm">
                v{{ plugin.version }}
            </span>
            <Tag :value="pluginStatus" :severity="statusSeverity" class="ml-2" />
            <span v-if="pluginStatusReason" class="text-text-color-secondary text-sm italic">
                {{ pluginStatusReason }}
            </span>

            <div class="flex-1" />

            <!-- Lifecycle controls (integration plugins only) -->
            <template v-if="isManaged">
                <Button
                    v-tooltip.bottom="t('layout.plugins.start')"
                    class="!p-1.5"
                    :disabled="pluginStatus === PluginStatus.Running"
                    severity="success"
                    text
                    @click="startPlugin"
                >
                    <svg-icon type="mdi" :path="mdiPlay" :size="deviceStore.getREMSize(1.25)" />
                </Button>
                <Button
                    v-tooltip.bottom="t('layout.plugins.stop')"
                    class="!p-1.5"
                    :disabled="pluginStatus === PluginStatus.Stopped"
                    severity="danger"
                    text
                    @click="stopPlugin"
                >
                    <svg-icon type="mdi" :path="mdiStop" :size="deviceStore.getREMSize(1.25)" />
                </Button>
                <Button
                    v-tooltip.bottom="t('layout.plugins.restart')"
                    class="!p-1.5"
                    text
                    @click="restartPlugin"
                >
                    <svg-icon type="mdi" :path="mdiRestart" :size="deviceStore.getREMSize(1.25)" />
                </Button>
            </template>

            <!-- Settings gear -->
            <Button
                v-if="hasSettingsUi"
                v-tooltip.bottom="t('layout.topbar.settings')"
                class="!p-1.5"
                text
                @click="openSettingsModal"
            >
                <svg-icon type="mdi" :path="mdiCogs" :size="deviceStore.getREMSize(1.25)" />
            </Button>
        </div>

        <!-- Full-page iframe or info page -->
        <div v-if="hasFullPageUi" class="flex-1 min-h-0">
            <iframe
                :ref="(el: any) => { pluginIframe.iframeRef.value = el }"
                :name="`iframe-fullpage-${pluginId}`"
                :src="pluginIframe.pluginUrl('app.html')"
                class="w-full h-full border-0"
                sandbox="allow-scripts"
            >
                This browser does not support iframes.
            </iframe>
        </div>
        <div v-else class="flex-1 p-6 overflow-auto">
            <!-- Info/management page for plugins without app.html -->
            <div class="max-w-2xl mx-auto space-y-4">
                <div v-if="plugin.description" class="text-wrap italic text-text-color-secondary">
                    {{ plugin.description }}
                </div>
                <table class="bg-bg-two rounded-lg w-full">
                    <tbody>
                        <tr class="border-b border-border-one">
                            <td class="py-3 px-4 font-semibold w-40">{{ t('layout.plugins.type') }}</td>
                            <td class="py-3 px-4">{{ plugin.service_type }}</td>
                        </tr>
                        <tr v-if="plugin.address" class="border-b border-border-one">
                            <td class="py-3 px-4 font-semibold">{{ t('layout.plugins.address') }}</td>
                            <td class="py-3 px-4"><code>{{ plugin.address }}</code></td>
                        </tr>
                        <tr class="border-b border-border-one">
                            <td class="py-3 px-4 font-semibold">{{ t('layout.plugins.privileges') }}</td>
                            <td class="py-3 px-4" :class="{ 'font-bold': plugin.privileged }">
                                {{ plugin.privileged ? t('layout.settings.plugins.privileged') : t('layout.settings.plugins.restricted') }}
                            </td>
                        </tr>
                        <tr v-if="plugin.url">
                            <td class="py-3 px-4 font-semibold">{{ t('layout.plugins.url') }}</td>
                            <td class="py-3 px-4">
                                <a class="inline-flex items-center gap-1 underline" :href="plugin.url" target="_blank" rel="noopener noreferrer">
                                    <svg-icon type="mdi" :path="mdiLinkVariant" :size="deviceStore.getREMSize(1)" />
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
