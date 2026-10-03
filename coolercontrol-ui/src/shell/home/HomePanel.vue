<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { v4 as uuidV4 } from 'uuid'
import {
    mdiBellPlusOutline,
    mdiDragVertical,
    mdiFan,
    mdiFanAlert,
    mdiHomeOutline,
    mdiPinOff,
    mdiTextBoxOutline,
    mdiViewDashboardOutline,
} from '@mdi/js'
import { VueDraggable } from 'vue-draggable-plus'
import { storeToRefs } from 'pinia'
import { ref, watchEffect } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import type { RouteLocationRaw } from 'vue-router'
import { type Color, type Device, type UID } from '@/models/Device.ts'
import { Dashboard } from '@/models/Dashboard.ts'
import { ChannelMetric } from '@/models/ChannelSource.ts'
import { useFailAlert } from '@/composables/useFailAlert.ts'
import PanelHeader from '@/shell/PanelHeader.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { coolingChannels, pinId } from '@/shell/cooling/channels.ts'
import { reorderSubset } from '@/shell/panelOrder.ts'
import { monitoringSensors } from '@/shell/monitoring/sensors.ts'
import { channelKind, channelKindIcon, channelSpins } from '@/shell/channelIcon.ts'
import { channelRoute } from '@/shell/channelRoute.ts'
import CCColorPicker from '@/components/CCColorPicker.vue'
import TagPopover from '@/shell/monitoring/TagPopover.vue'
import UiSeparator from '@/shell/ui/UiSeparator.vue'
import { useRouteActive } from '@/shell/routeActive.ts'
import { groupDigits } from '@/shell/digitGroups.ts'
import ChannelHealthIcon from '@/shell/ChannelHealthIcon.vue'

const { t } = useI18n()
const router = useRouter()
const { createFailAlert } = useFailAlert()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)

// The Home panel shows everything pinned anywhere: fans open their cooling
// page, custom sensors their editor under Devices, other sensors their
// monitoring page, dashboards their dashboard. Channel rows carry the same
// hover actions as their Cooling/Monitoring panel rows; dashboards only unpin.
interface PinnedRow {
    key: string
    label: string
    sublabel?: string
    color?: string
    value?: string
    icon?: string
    spins?: boolean
    to: RouteLocationRaw
    channel?: { deviceUID: UID; channelName: string }
    alertKind?: 'temp' | 'fan'
}

const label = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.name ??
    channelName

const color = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.color ??
    ''

const deviceLabel = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.name ?? deviceUID

const liveValue = (deviceUID: UID, channelName: string): string => {
    const values = currentDeviceStatus.value.get(deviceUID)?.get(channelName)
    if (values == null) return ''
    if (values.duty != null) return `${values.duty} ${t('common.percentUnit')}`
    if (values.temp != null) return `${values.temp} ${t('common.tempUnit')}`
    if (values.watts != null) return `${values.watts} ${t('common.wattAbbr')}`
    if (values.freq != null) {
        const precision = settingsStore.frequencyPrecision
        return precision === 1
            ? `${values.freq} ${t('common.mhzAbbr')}`
            : `${(Number(values.freq) / precision).toFixed(2)} ${t('common.ghzAbbr')}`
    }
    if (values.rpm != null) {
        return `${groupDigits(values.rpm)} ${settingsStore.rpmUnit(deviceUID, channelName)}`
    }
    return ''
}

const buildPinnedRows = (): PinnedRow[] => {
    const rows: PinnedRow[] = []
    const seen = new Set<string>()
    const dashboardsByUid = new Map<string, Dashboard>(
        settingsStore.dashboards.map((dashboard) => [dashboard.uid, dashboard]),
    )
    const devicesByUid = new Map<UID, Device>()
    for (const device of deviceStore.allDevices()) devicesByUid.set(device.uid, device)
    const fanIds = new Set(
        coolingChannels(deviceStore.allDevices(), settingsStore.channelUnit).flatMap((group) =>
            group.channels.map((channel) => pinId(channel.deviceUID, channel.channelName)),
        ),
    )
    const sensorIds = new Set(
        monitoringSensors(deviceStore.allDevices()).flatMap((group) =>
            group.sensors.map((sensor) => pinId(sensor.deviceUID, sensor.channelName)),
        ),
    )
    for (const id of settingsStore.pinnedIds) {
        if (seen.has(id)) continue
        seen.add(id)
        const dashboard = dashboardsByUid.get(id)
        if (dashboard != null) {
            rows.push({
                key: id,
                label: dashboard.name,
                to: { name: 'monitoring-dashboard', params: { dashboardUID: dashboard.uid } },
            })
            continue
        }
        const separator = id.indexOf('_')
        if (separator === -1) continue
        const deviceUID = id.slice(0, separator)
        const channelName = id.slice(separator + 1)
        const values = currentDeviceStatus.value.get(deviceUID)?.get(channelName)
        const base = {
            key: id,
            label: label(deviceUID, channelName),
            sublabel: deviceLabel(deviceUID),
            color: color(deviceUID, channelName),
            value: liveValue(deviceUID, channelName),
            channel: { deviceUID, channelName },
            to: channelRoute(
                deviceStore.allDevices(),
                deviceUID,
                channelName,
                settingsStore.channelUnit,
            ),
        }
        if (fanIds.has(id)) {
            rows.push({
                ...base,
                icon: mdiFan,
                spins: channelSpins('fan', values, settingsStore.eyeCandy),
                alertKind: values?.rpm != null ? 'fan' : undefined,
            })
        } else if (sensorIds.has(id)) {
            const kind = channelKind(
                devicesByUid.get(deviceUID),
                channelName,
                values,
                settingsStore.channelUnit,
            )
            rows.push({
                ...base,
                icon: channelKindIcon(kind),
                alertKind: kind === 'temp' ? 'temp' : undefined,
            })
        }
    }
    return rows
}
const pinnedRows = ref<PinnedRow[]>([])
watchEffect(() => {
    pinnedRows.value = buildPinnedRows()
})
const persistPinnedOrder = (): void => {
    settingsStore.pinnedIds = reorderSubset(
        settingsStore.pinnedIds,
        pinnedRows.value.map((row) => row.key),
    )
}

// Keeps a row's hover actions visible while its tag popover is open.
const openTagRow = ref<string | null>(null)
const onTagOpen = (rowKey: string, open: boolean): void => {
    openTagRow.value = open ? rowKey : null
}

const unpin = (row: PinnedRow): void => {
    settingsStore.pinnedIds = settingsStore.pinnedIds.filter((pinned) => pinned !== row.key)
}

const setRowColor = (row: PinnedRow, newColor: Color): void => {
    if (row.channel == null) return
    const setting = settingsStore.allUIDeviceSettings
        .get(row.channel.deviceUID)
        ?.sensorsAndChannels.get(row.channel.channelName)
    if (setting != null) setting.userColor = newColor
}

const createAlert = (row: PinnedRow): void => {
    if (row.alertKind == null || row.channel == null) return
    const { deviceUID, channelName } = row.channel
    if (row.alertKind === 'fan') {
        createFailAlert(deviceUID, channelName, row.label)
        return
    }
    router.push({
        name: 'monitoring-alert-new',
        // `key` forces a remount: the path is identical between two create-alert clicks,
        // so without it the editor keeps the previous sensor's prefill.
        query: {
            device: deviceUID,
            channel: channelName,
            metric: ChannelMetric.Temp,
            key: uuidV4(),
        },
    })
}

const isRouteActive = useRouteActive()
</script>

<template>
    <div class="flex flex-col gap-0.5 p-2 pb-24 text-base">
        <RouterLink
            :to="{ name: 'section-home' }"
            class="flex items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
            exact-active-class="bg-surface-hover !text-accent"
        >
            <svg-icon
                type="mdi"
                :path="mdiHomeOutline"
                :size="18"
                class="shrink-0 text-text-color-secondary"
            />
            {{ t('layout.shell.homePanel.overview') }}
        </RouterLink>
        <RouterLink
            :to="{ name: 'home-logs' }"
            class="flex items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
            exact-active-class="bg-surface-hover !text-accent"
        >
            <svg-icon
                type="mdi"
                :path="mdiTextBoxOutline"
                :size="18"
                class="shrink-0 text-text-color-secondary"
            />
            {{ t('layout.shell.homePanel.logs') }}
        </RouterLink>

        <template v-if="pinnedRows.length > 0">
            <UiSeparator class="my-1" />
            <PanelHeader :label="t('layout.menu.pinned')" />
            <VueDraggable
                :force-auto-scroll-fallback="true"
                v-model="pinnedRows"
                handle=".drag-handle"
                :animation="150"
                class="flex flex-col gap-0.5"
                data-panel-pinned
                @end="persistPinnedOrder"
            >
                <div
                    v-for="row in pinnedRows"
                    :key="row.key"
                    class="group flex items-center rounded-lg hover:bg-surface-hover has-[:focus-visible]:bg-surface-hover has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent"
                    :class="{ 'bg-surface-hover': isRouteActive(row.to) }"
                >
                    <RouterLink
                        :to="row.to"
                        class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none"
                        exact-active-class="!text-accent"
                    >
                        <svg-icon
                            v-if="row.icon"
                            type="mdi"
                            :path="row.icon"
                            :size="18"
                            class="shrink-0"
                            :class="{ 'animate-spin-slow': row.spins }"
                            :style="{ color: row.color || undefined }"
                        />
                        <svg-icon
                            v-else
                            type="mdi"
                            :path="mdiViewDashboardOutline"
                            :size="18"
                            class="shrink-0 text-text-color-secondary"
                        />
                        <span class="truncate">{{ row.label }}</span>
                        <!-- Huge shrink so the sublabel (device name) fully collapses
                             before the label (truncate, shrink-1) gives up any space. -->
                        <span
                            v-if="row.sublabel"
                            class="shrink-[9999] truncate text-xs text-text-color-secondary"
                        >
                            {{ row.sublabel }}
                        </span>
                        <ChannelHealthIcon
                            v-if="row.channel != null"
                            :device-u-i-d="row.channel.deviceUID"
                            :channel-name="row.channel.channelName"
                        />
                        <span
                            v-if="row.value"
                            class="ml-auto whitespace-nowrap font-numeric tabular-nums text-text-color group-hover:hidden group-has-[:focus-visible]:hidden"
                            :class="{ '!hidden': openTagRow === row.key }"
                        >
                            {{ row.value }}
                        </span>
                    </RouterLink>
                    <div
                        class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex"
                        :class="{ '!flex': openTagRow === row.key }"
                    >
                        <template v-if="row.channel != null">
                            <button
                                v-if="row.alertKind != null"
                                type="button"
                                class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                v-tooltip.top="
                                    row.alertKind === 'fan'
                                        ? t('layout.shell.monitoringPanel.failAlert')
                                        : t('layout.shell.monitoringPanel.createAlert')
                                "
                                @click.prevent="createAlert(row)"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="
                                        row.alertKind === 'fan' ? mdiFanAlert : mdiBellPlusOutline
                                    "
                                    :size="16"
                                />
                            </button>
                            <TagPopover
                                :device-u-i-d="row.channel.deviceUID"
                                :channel-name="row.channel.channelName"
                                @open="(open: boolean) => onTagOpen(row.key, open)"
                            />
                            <span class="flex w-6 shrink-0 justify-center">
                                <CCColorPicker
                                    :model-value="row.color ?? ''"
                                    :size="1.25"
                                    @update:model-value="(c: Color) => setRowColor(row, c)"
                                />
                            </span>
                        </template>
                        <button
                            type="button"
                            class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                            v-tooltip.top="t('layout.shell.coolingPanel.unpin')"
                            @click.prevent="unpin(row)"
                        >
                            <svg-icon type="mdi" :path="mdiPinOff" :size="16" />
                        </button>
                        <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                            <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                        </span>
                    </div>
                </div>
            </VueDraggable>
        </template>
    </div>
</template>
