<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiDragVertical,
    mdiPlay,
    mdiPowerPlugOutline,
    mdiRestart,
    mdiStop,
    mdiViewListOutline,
} from '@mdi/js'
import { VueDraggable } from 'vue-draggable-plus'
import { onMounted, ref, watchEffect } from 'vue'
import { useI18n } from 'vue-i18n'
import { PluginStatus } from '@/models/Plugins.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { usePluginActions } from '@/composables/usePluginActions.ts'
import { orderedByGroup, setGroupOrder } from '@/shell/panelOrder.ts'
import { useRouteActive } from '@/shell/routeActive.ts'
import type { RouteLocationRaw } from 'vue-router'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const pluginActions = usePluginActions()

interface PluginRow {
    id: string
    disabled: boolean
}

// Plugins get their own order group; no overlap with device/entity orders.
const pluginRows = ref<PluginRow[]>([])
watchEffect(() => {
    pluginRows.value = orderedByGroup(
        settingsStore.menuOrder,
        'plugins',
        deviceStore.plugins.map((plugin) => ({
            id: plugin.id,
            disabled: plugin.disabled === true,
        })),
        (row) => row.id,
    )
})
const persistPluginOrder = (): void => {
    settingsStore.menuOrder = setGroupOrder(
        settingsStore.menuOrder,
        'plugins',
        pluginRows.value.map((row) => row.id),
    )
}

// Live statuses drive the row dot and the start/stop/restart hover actions.
const statusOf = (pluginId: string): PluginStatus | undefined =>
    pluginActions.statuses.value.get(pluginId)?.status
onMounted(pluginActions.refreshStatuses)

const statusDotClass = (row: PluginRow): string => {
    if (row.disabled) return 'bg-warning'
    switch (statusOf(row.id)) {
        case PluginStatus.Running:
            return 'bg-success'
        case PluginStatus.Stopped:
            return 'bg-error'
        default:
            return 'bg-text-color-secondary'
    }
}

const busy = ref<Set<string>>(new Set())
const runAction = async (
    pluginId: string,
    action: (id: string) => Promise<void>,
): Promise<void> => {
    busy.value = new Set(busy.value).add(pluginId)
    await action(pluginId)
    const next = new Set(busy.value)
    next.delete(pluginId)
    busy.value = next
}

const actionButtonClasses =
    'rounded p-1 text-text-color-secondary outline-none hover:text-text-color ' +
    'focus-visible:ring-2 focus-visible:ring-accent disabled:pointer-events-none disabled:opacity-50'

// The row wrapper needs the same target its link uses, so both read it here.
const pluginTarget = (pluginId: string): RouteLocationRaw => ({
    name: 'plugin-page',
    params: { pluginId },
})
const isRouteActive = useRouteActive()
</script>

<template>
    <div class="flex flex-col gap-0.5 p-2 pb-24 text-base">
        <RouterLink
            :to="{ name: 'plugins-overview' }"
            class="flex items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
            exact-active-class="bg-surface-hover !text-accent"
        >
            <svg-icon
                type="mdi"
                :path="mdiViewListOutline"
                :size="18"
                class="shrink-0 text-text-color-secondary"
            />
            {{ t('layout.plugins.overview') }}
        </RouterLink>

        <!-- Every plugin links to its page: it doubles as the info/status
          page; the iframe UI only renders there when the plugin provides one. -->
        <VueDraggable
            :force-auto-scroll-fallback="true"
            v-model="pluginRows"
            handle=".drag-handle"
            :animation="150"
            class="flex flex-col gap-0.5"
            @end="persistPluginOrder"
        >
            <div
                v-for="plugin in pluginRows"
                :key="plugin.id"
                class="group flex items-center rounded-lg hover:bg-surface-hover has-[:focus-visible]:bg-surface-hover has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent"
                :class="{ 'bg-surface-hover': isRouteActive(pluginTarget(plugin.id)) }"
            >
                <RouterLink
                    :to="pluginTarget(plugin.id)"
                    class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 outline-none"
                    :class="
                        plugin.disabled ? 'text-text-color-secondary opacity-60' : 'text-text-color'
                    "
                    exact-active-class="!text-accent"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiPowerPlugOutline"
                        :size="18"
                        class="shrink-0 text-text-color-secondary"
                    />
                    <span class="truncate">{{ plugin.id }}</span>
                    <span
                        class="ml-1 h-2 w-2 shrink-0 rounded-full"
                        :class="statusDotClass(plugin)"
                    />
                    <span
                        v-if="plugin.disabled"
                        class="ml-auto text-xs group-hover:hidden group-has-[:focus-visible]:hidden"
                    >
                        {{ t('models.pluginStatus.disabled') }}
                    </span>
                </RouterLink>
                <div
                    class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex"
                >
                    <template v-if="!plugin.disabled">
                        <button
                            v-if="statusOf(plugin.id) === PluginStatus.Stopped"
                            type="button"
                            :class="actionButtonClasses"
                            :disabled="busy.has(plugin.id)"
                            v-tooltip.top="t('layout.plugins.start')"
                            @click.prevent="runAction(plugin.id, pluginActions.startPlugin)"
                        >
                            <svg-icon type="mdi" :path="mdiPlay" :size="16" />
                        </button>
                        <button
                            v-if="statusOf(plugin.id) === PluginStatus.Running"
                            type="button"
                            :class="actionButtonClasses"
                            :disabled="busy.has(plugin.id)"
                            v-tooltip.top="t('layout.plugins.stop')"
                            @click.prevent="runAction(plugin.id, pluginActions.stopPlugin)"
                        >
                            <svg-icon type="mdi" :path="mdiStop" :size="16" />
                        </button>
                        <button
                            v-if="statusOf(plugin.id) === PluginStatus.Running"
                            type="button"
                            :class="actionButtonClasses"
                            :disabled="busy.has(plugin.id)"
                            v-tooltip.top="t('layout.plugins.restart')"
                            @click.prevent="runAction(plugin.id, pluginActions.restartPlugin)"
                        >
                            <svg-icon type="mdi" :path="mdiRestart" :size="16" />
                        </button>
                    </template>
                    <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                        <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                    </span>
                </div>
            </div>
        </VueDraggable>
    </div>
</template>
