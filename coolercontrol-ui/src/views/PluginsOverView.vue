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
    mdiAlertCircleOutline,
    mdiBookOpenPageVariantOutline,
    mdiLinkVariant,
    mdiPowerPlugOutline,
} from '@mdi/js'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import DataTable, { DataTableRowSelectEvent } from 'primevue/datatable'
import Column from 'primevue/column'
import Tag from 'primevue/tag'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { getPluginStatusDisplayName, PluginStatus } from '@/models/Plugins.ts'

const STATUS_POLL_INTERVAL_MS = 30_000

const deviceStore = useDeviceStore()
const router = useRouter()
const { t } = useI18n()

const pluginStatuses = ref<Map<string, PluginStatus>>(new Map())
let statusPollTimer: ReturnType<typeof setInterval> | undefined

const pluginsList = computed(() => {
    return deviceStore.plugins.map((plugin) => ({
        ...plugin,
        status: pluginStatuses.value.get(plugin.id) ?? PluginStatus.Unmanaged,
    }))
})

const statusSeverity = (status: PluginStatus): 'success' | 'danger' | 'secondary' => {
    switch (status) {
        case PluginStatus.Running:
            return 'success'
        case PluginStatus.Stopped:
            return 'danger'
        default:
            return 'secondary'
    }
}

const loadStatuses = async (): Promise<void> => {
    const statuses = new Map<string, PluginStatus>()
    for (const plugin of deviceStore.plugins) {
        const statusDto = await deviceStore.daemonClient.getPluginStatus(plugin.id)
        statuses.set(plugin.id, statusDto.status as PluginStatus)
    }
    pluginStatuses.value = statuses
}

const onRowSelect = (event: DataTableRowSelectEvent) => {
    router.push({ name: 'plugin-page', params: { pluginId: event.data.id } })
}

onMounted(async () => {
    await loadStatuses()
    statusPollTimer = setInterval(loadStatuses, STATUS_POLL_INTERVAL_MS)
})

onUnmounted(() => {
    if (statusPollTimer != null) {
        clearInterval(statusPollTimer)
    }
})
</script>

<template>
    <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl font-bold">{{ t('layout.plugins.overview') }}</div>
    </div>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="p-4 pb-16 h-screen w-full whitespace-normal">
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
                        :path="mdiBookOpenPageVariantOutline"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                    {{ t('layout.plugins.docsLink') }}
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
                <DataTable
                    :value="pluginsList"
                    selection-mode="single"
                    data-key="id"
                    :meta-key-selection="false"
                    @row-select="onRowSelect"
                >
                    <template #empty>
                        <div class="flex items-center gap-2 text-text-color-secondary py-4">
                            <svg-icon
                                type="mdi"
                                :path="mdiPowerPlugOutline"
                                :size="deviceStore.getREMSize(1.25)"
                            />
                            {{ t('layout.plugins.noPlugins') }}
                        </div>
                    </template>
                    <Column field="id" :header="t('common.name')" body-class="underline" />
                    <Column field="service_type" :header="t('layout.plugins.type')" />
                    <Column field="status" :header="t('common.state')">
                        <template #body="slotProps">
                            <Tag
                                :value="getPluginStatusDisplayName(slotProps.data.status)"
                                :severity="statusSeverity(slotProps.data.status)"
                            />
                        </template>
                    </Column>
                    <Column field="version" :header="t('views.appInfo.version')">
                        <template #body="slotProps">
                            {{ slotProps.data.version ?? '-' }}
                        </template>
                    </Column>
                    <Column field="description" :header="t('layout.plugins.description')">
                        <template #body="slotProps">
                            <span class="text-text-color-secondary text-sm">
                                {{ slotProps.data.description ?? '-' }}
                            </span>
                        </template>
                    </Column>
                    <Column field="privileged" :header="t('layout.plugins.privileges')">
                        <template #body="slotProps">
                            <span :class="{ 'font-bold': slotProps.data.privileged }">
                                {{
                                    slotProps.data.privileged
                                        ? t('layout.settings.plugins.privileged')
                                        : t('layout.settings.plugins.restricted')
                                }}
                            </span>
                        </template>
                    </Column>
                </DataTable>
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
</template>

<style scoped lang="scss"></style>
