<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiAlertCircleOutline, mdiLinkVariant, mdiPowerPlugOutline } from '@mdi/js'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'reka-ui'
import UiTable from '@/shell/ui/UiTable.vue'
import UiTag from '@/shell/ui/UiTag.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { usePluginActions } from '@/composables/usePluginActions.ts'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { computed, onMounted, onUnmounted } from 'vue'
import { getPluginStatusDisplayName, PluginStatus } from '@/models/Plugins.ts'

const STATUS_POLL_INTERVAL_MS = 30_000

const deviceStore = useDeviceStore()
const router = useRouter()
const { t } = useI18n()
const { statuses, refreshStatuses } = usePluginActions()

let statusPollTimer: ReturnType<typeof setInterval> | undefined

const pluginsList = computed(() => {
    return deviceStore.plugins.map((plugin) => ({
        ...plugin,
        status: statuses.value.get(plugin.id)?.status ?? PluginStatus.Unmanaged,
    }))
})

const statusSeverity = (
    status: PluginStatus,
    disabled: boolean,
): 'success' | 'danger' | 'secondary' | 'warn' => {
    if (disabled) return 'warn'
    switch (status) {
        case PluginStatus.Running:
            return 'success'
        case PluginStatus.Stopped:
            return 'danger'
        default:
            return 'secondary'
    }
}

const statusDisplayName = (status: PluginStatus, disabled: boolean): string => {
    if (disabled) return t('models.pluginStatus.disabled')
    return getPluginStatusDisplayName(status)
}

const onRowSelect = (plugin: { id: string }) => {
    router.push({ name: 'plugin-page', params: { pluginId: plugin.id } })
}

onMounted(async () => {
    await deviceStore.loadAllPlugins()
    await refreshStatuses()
    statusPollTimer = setInterval(refreshStatuses, STATUS_POLL_INTERVAL_MS)
})

onUnmounted(() => {
    if (statusPollTimer != null) {
        clearInterval(statusPollTimer)
    }
})
</script>

<template>
    <div class="flex h-full flex-col">
        <div class="flex shrink-0 flex-wrap items-center gap-3 px-4 pt-4">
            <h1 class="text-xl font-semibold text-text-color">
                {{ t('layout.plugins.overview') }}
            </h1>
        </div>
        <ScrollAreaRoot class="min-h-0 flex-1" style="--scrollbar-size: 10px">
            <ScrollAreaViewport class="p-4 h-full w-full whitespace-normal">
                <!-- Getting Started -->
                <div class="mt-8 flex flex-col max-w-3xl">
                    <span class="pb-3 ml-1 font-semibold text-xl text-text-color">
                        {{ t('layout.plugins.plugins') }}
                    </span>
                    <p class="ml-1 text-text-color-secondary leading-relaxed">
                        {{ t('layout.plugins.gettingStarted') }}
                    </p>
                    <a
                        class="mt-3 ml-1 inline-flex items-center gap-1.5 underline text-text-color"
                        href="https://docs.coolercontrol.org/automation/plugins.html"
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiPowerPlugOutline"
                            :size="deviceStore.getREMSize(1.25)"
                        />
                        {{ t('layout.plugins.findPlugins') }}
                        <svg-icon
                            type="mdi"
                            :path="mdiLinkVariant"
                            :size="deviceStore.getREMSize(0.875)"
                        />
                    </a>
                </div>

                <!-- Info notes -->
                <div class="mt-6 flex flex-col gap-3 max-w-3xl ml-1">
                    <div class="flex items-start gap-2 text-text-color-secondary">
                        <svg-icon
                            type="mdi"
                            class="shrink-0 mt-0.5"
                            :path="mdiAlertCircleOutline"
                            :size="deviceStore.getREMSize(1.25)"
                        />
                        <span>{{ t('layout.plugins.restartNote') }}</span>
                    </div>
                    <div class="flex items-start gap-2 text-text-color-secondary">
                        <svg-icon
                            type="mdi"
                            class="shrink-0 mt-0.5"
                            :path="mdiAlertCircleOutline"
                            :size="deviceStore.getREMSize(1.25)"
                        />
                        <span>{{ t('layout.plugins.containerNote') }}</span>
                    </div>
                </div>

                <!-- Installed Plugins -->
                <div class="mt-8 flex flex-col">
                    <span class="pb-3 ml-1 font-semibold text-xl text-text-color">
                        {{ t('layout.plugins.installedPlugins') }}
                    </span>
                    <UiTable bordered>
                        <template #head>
                            <tr>
                                <th>{{ t('common.name') }}</th>
                                <th>{{ t('layout.plugins.type') }}</th>
                                <th>{{ t('common.state') }}</th>
                                <th>{{ t('views.appInfo.version') }}</th>
                                <th>{{ t('layout.plugins.description') }}</th>
                                <th>{{ t('layout.plugins.privileges') }}</th>
                            </tr>
                        </template>
                        <tr v-if="pluginsList.length === 0">
                            <td colspan="6">
                                <div class="flex items-center gap-2 text-text-color-secondary py-4">
                                    <svg-icon
                                        type="mdi"
                                        :path="mdiPowerPlugOutline"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                    {{ t('layout.plugins.noPlugins') }}
                                </div>
                            </td>
                        </tr>
                        <tr
                            v-for="plugin in pluginsList"
                            :key="plugin.id"
                            class="cursor-pointer hover:bg-surface-hover"
                            @click="onRowSelect(plugin)"
                        >
                            <td class="underline">{{ plugin.id }}</td>
                            <td>{{ plugin.service_type }}</td>
                            <td>
                                <UiTag
                                    :value="statusDisplayName(plugin.status, plugin.disabled)"
                                    :severity="statusSeverity(plugin.status, plugin.disabled)"
                                />
                            </td>
                            <td>{{ plugin.version ?? '-' }}</td>
                            <td>
                                <span class="text-text-color-secondary text-sm">
                                    {{ plugin.description ?? '-' }}
                                </span>
                            </td>
                            <td>
                                <span :class="{ 'font-bold': plugin.privileged }">
                                    {{
                                        plugin.privileged
                                            ? t('layout.settings.plugins.privileged')
                                            : t('layout.settings.plugins.restricted')
                                    }}
                                </span>
                            </td>
                        </tr>
                    </UiTable>
                </div>
            </ScrollAreaViewport>
            <ScrollAreaScrollbar
                class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
                orientation="vertical"
            >
                <ScrollAreaThumb
                    class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
                />
            </ScrollAreaScrollbar>
        </ScrollAreaRoot>
    </div>
</template>

<style scoped lang="scss"></style>
