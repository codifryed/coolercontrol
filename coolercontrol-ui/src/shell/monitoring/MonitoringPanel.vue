<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { v4 as uuidV4 } from 'uuid'
import {
    mdiAlert,
    mdiBellOffOutline,
    mdiBellOutline,
    mdiBellPlusOutline,
    mdiBellSleepOutline,
    mdiDragVertical,
    mdiFanAlert,
    mdiHome,
    mdiPinOff,
    mdiPinOutline,
    mdiPlus,
    mdiViewDashboardOutline,
} from '@mdi/js'
import { VueDraggable } from 'vue-draggable-plus'
import AlertSilenceMenu from '@/components/AlertSilenceMenu.vue'
import { storeToRefs } from 'pinia'
import { computed, ref, watchEffect } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import type { Color, Device, UID } from '@/models/Device.ts'
import { Dashboard } from '@/models/Dashboard.ts'
import { Alert, alertIsSilenced, getAlertStateClass, getAlertStateIcon } from '@/models/Alert.ts'
import { ChannelMetric } from '@/models/ChannelSource.ts'
import { useFailAlert } from '@/composables/useFailAlert.ts'
import { useDashboardActions } from '@/composables/useDashboardActions.ts'
import CCColorPicker from '@/components/CCColorPicker.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { pinId } from '@/shell/cooling/channels.ts'
import {
    orderedByGroup,
    orderPinnedRows,
    reorderSubset,
    reorderTopLevel,
    setDeviceChildrenSubset,
    setGroupOrder,
} from '@/shell/panelOrder.ts'
import { monitoringSensors, type MonitoringSensor } from '@/shell/monitoring/sensors.ts'
import { channelKind, channelKindIcon, channelSpins } from '@/shell/channelIcon.ts'
import { channelRoute, monitoringChannelRoute } from '@/shell/channelRoute.ts'
import PanelHeader from '@/shell/PanelHeader.vue'
import TagPopover from '@/shell/monitoring/TagPopover.vue'
import TagChips from '@/shell/TagChips.vue'
import UiTooltip from '@/shell/ui/UiTooltip.vue'
import UiSeparator from '@/shell/ui/UiSeparator.vue'
import { useRouteActive } from '@/shell/routeActive.ts'
import type { RouteLocationRaw } from 'vue-router'

const { t } = useI18n()
const router = useRouter()
const { createFailAlert } = useFailAlert()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)

// Mutable copies so rows are drag-sortable; rebuilt when devices change.
const groups = ref<ReturnType<typeof monitoringSensors>>([])
watchEffect(() => {
    groups.value = monitoringSensors(deviceStore.allDevices())
})

const allChannelIds = (deviceUID: UID): string[] => {
    const device = [...deviceStore.allDevices()].find((dev) => dev.uid === deviceUID)
    if (device?.info == null) return []
    const ids = [...device.info.temps.keys(), ...device.info.channels.keys()]
    return ids.map((name) => pinId(deviceUID, name))
}

const persistSensorOrder = (group: { deviceUID: UID; sensors: MonitoringSensor[] }): void => {
    settingsStore.menuOrder = setDeviceChildrenSubset(
        settingsStore.menuOrder,
        group.deviceUID,
        group.sensors.map((sensor) => pinId(sensor.deviceUID, sensor.channelName)),
        allChannelIds(group.deviceUID),
    )
    deviceStore.reSortDevicesByMenuOrder()
}

const deviceLabel = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.name ?? deviceUID

const deviceColor = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.userColor ?? ''

const colorStore = useThemeColorsStore()
const devicePickerColor = (deviceUID: UID): string =>
    deviceColor(deviceUID) || `rgb(${colorStore.themeColors.text_color})`
const setDeviceColor = (deviceUID: UID, newColor: Color): void => {
    const setting = settingsStore.allUIDeviceSettings.get(deviceUID)
    if (setting != null) setting.userColor = newColor
}

// Held open while the color popover is: the pointer leaves the header for the
// portalled content, which would otherwise hide the trigger under it.
const openColorDevice = ref<UID | undefined>(undefined)

// The header names a device, so it goes where the device does, and carries the
// same two actions its row in the Devices panel has.
const deviceTarget = (deviceUID: UID): RouteLocationRaw => ({
    name: 'devices-device',
    params: { deviceUID },
})
// One device order, shared by every panel. Only the devices this panel lists
// move within it; the rest keep their slots.
const persistDeviceOrder = (): void => {
    settingsStore.menuOrder = reorderTopLevel(
        settingsStore.menuOrder,
        groups.value.map((group) => group.deviceUID),
    )
    deviceStore.reSortDevicesByMenuOrder()
}

const sensorLabel = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.name ??
    channelName

const sensorColor = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.color ??
    ''

const devicesByUid = computed(() => {
    const map = new Map<UID, Device>()
    for (const device of deviceStore.allDevices()) map.set(device.uid, device)
    return map
})
const sensorValues = (sensor: MonitoringSensor) =>
    currentDeviceStatus.value.get(sensor.deviceUID)?.get(sensor.channelName)
const sensorIcon = (sensor: MonitoringSensor): string =>
    channelKindIcon(
        channelKind(
            devicesByUid.value.get(sensor.deviceUID),
            sensor.channelName,
            sensorValues(sensor),
        ),
    )
const sensorSpins = (sensor: MonitoringSensor): boolean => {
    const values = sensorValues(sensor)
    return channelSpins(
        channelKind(devicesByUid.value.get(sensor.deviceUID), sensor.channelName, values),
        values,
        settingsStore.eyeCandy,
    )
}

// Primary live value: temp > watts > freq > load(duty) > rpm; formats follow
// SensorTable's conventions.
const liveValue = (sensor: MonitoringSensor): string => {
    const values = currentDeviceStatus.value.get(sensor.deviceUID)?.get(sensor.channelName)
    if (values == null) return ''
    if (values.temp != null) return `${values.temp} ${t('common.tempUnit')}`
    if (values.watts != null) return `${values.watts} ${t('common.wattAbbr')}`
    if (values.freq != null) {
        const precision = settingsStore.frequencyPrecision
        return precision === 1
            ? `${values.freq} ${t('common.mhzAbbr')}`
            : `${(Number(values.freq) / precision).toFixed(2)} ${t('common.ghzAbbr')}`
    }
    if (values.duty != null) return `${values.duty} ${t('common.percentUnit')}`
    if (values.rpm != null) return `${values.rpm} ${t('common.rpmAbbr')}`
    return ''
}

const isDeviceUnreachable = (deviceUID: UID): boolean =>
    settingsStore.healthUnreachable.some((ref) => ref.device_uid === deviceUID)

// An unreachable device's channels also failsafe, so the device state is reported instead: it is
// the cause, and "not responding" is what the user can act on.
const healthTooltip = (deviceUID: UID, channelName?: string): string => {
    if (isDeviceUnreachable(deviceUID)) {
        return `${t('views.appInfo.deviceUnreachable')}: ${t('views.appInfo.deviceUnreachableDetail')}`
    }
    const ref = settingsStore.healthFailsafe.find(
        (entry) =>
            entry.device_uid === deviceUID && (channelName == null || entry.name === channelName),
    )
    const base = t('views.appInfo.failsafeActive')
    return ref?.reason ? `${base}: ${ref.reason}` : base
}

const isUnhealthy = (deviceUID: UID, channelName: string): boolean =>
    isDeviceUnreachable(deviceUID) ||
    settingsStore.healthFailsafe.some(
        (ref) => ref.device_uid === deviceUID && ref.name === channelName,
    )

const isPinned = (sensor: MonitoringSensor): boolean =>
    settingsStore.pinnedIds.includes(pinId(sensor.deviceUID, sensor.channelName))

const togglePin = (sensor: MonitoringSensor): void => {
    const id = pinId(sensor.deviceUID, sensor.channelName)
    settingsStore.pinnedIds = settingsStore.pinnedIds.includes(id)
        ? settingsStore.pinnedIds.filter((pinned) => pinned !== id)
        : [...settingsStore.pinnedIds, id]
}

// Alert convenience: temp sensors get a plain new-alert button; fan channels
// (those reporting rpm) get a "fail alert" prefilled to catch 0 rpm; other
// value types (duty/load/watts/freq) get no button.
type AlertKind = 'temp' | 'fan'
const alertKind = (sensor: MonitoringSensor): AlertKind | null => {
    if (sensor.isTemp) return 'temp'
    const values = currentDeviceStatus.value.get(sensor.deviceUID)?.get(sensor.channelName)
    return values?.rpm != null ? 'fan' : null
}
const createAlert = (sensor: MonitoringSensor): void => {
    const kind = alertKind(sensor)
    if (kind == null) return
    if (kind === 'fan') {
        createFailAlert(
            sensor.deviceUID,
            sensor.channelName,
            sensorLabel(sensor.deviceUID, sensor.channelName),
        )
        return
    }
    router.push({
        name: 'monitoring-alert-new',
        // `key` forces a remount: the path is identical between two create-alert clicks,
        // so without it the editor keeps the previous sensor's prefill.
        query: {
            device: sensor.deviceUID,
            channel: sensor.channelName,
            metric: ChannelMetric.Temp,
            key: uuidV4(),
        },
    })
}

const isDashboardPinned = (dashboard: Dashboard): boolean =>
    settingsStore.pinnedIds.includes(dashboard.uid)

const toggleDashboardPin = (dashboard: Dashboard): void =>
    void (settingsStore.pinnedIds = settingsStore.pinnedIds.includes(dashboard.uid)
        ? settingsStore.pinnedIds.filter((pinned) => pinned !== dashboard.uid)
        : [...settingsStore.pinnedIds, dashboard.uid])

// One list in the order things were pinned, as the Home panel shows them.
// Grouping by kind put a dashboard pinned last above sensors pinned first, and
// made a drag here mean something different from the same drag there.
type PinnedRow =
    | { kind: 'dashboard'; key: string; dashboard: Dashboard }
    | { kind: 'sensor'; key: string; sensor: MonitoringSensor }

const pinnedRows = ref<PinnedRow[]>([])
watchEffect(() => {
    const byId = new Map<string, PinnedRow>()
    for (const dashboard of settingsStore.dashboards) {
        byId.set(dashboard.uid, { kind: 'dashboard', key: dashboard.uid, dashboard })
    }
    for (const group of groups.value) {
        for (const sensor of group.sensors) {
            const key = pinId(sensor.deviceUID, sensor.channelName)
            byId.set(key, { kind: 'sensor', key, sensor })
        }
    }
    pinnedRows.value = orderPinnedRows(settingsStore.pinnedIds, byId)
})
const persistPinnedOrder = (): void => {
    settingsStore.pinnedIds = reorderSubset(
        settingsStore.pinnedIds,
        pinnedRows.value.map((row) => row.key),
    )
}

const setSensorColor = (sensor: MonitoringSensor, newColor: Color): void => {
    const setting = settingsStore.allUIDeviceSettings
        .get(sensor.deviceUID)
        ?.sensorsAndChannels.get(sensor.channelName)
    if (setting != null) setting.userColor = newColor
}
const sensorDefaultColor = (deviceUID: UID, channelName: string): Color | undefined =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)
        ?.defaultColor
// Reset clears the user override so the non-user-defined color applies again.
const resetSensorColor = (sensor: MonitoringSensor): void => {
    const setting = settingsStore.allUIDeviceSettings
        .get(sensor.deviceUID)
        ?.sensorsAndChannels.get(sensor.channelName)
    if (setting != null) setting.userColor = undefined
}

// Explicit drag order wins; otherwise home dashboard first, then store order.
const orderedDashboards = ref<Dashboard[]>([])
watchEffect(() => {
    const entry = settingsStore.menuOrder.find((item) => item.id === 'dashboards')
    if (entry?.children?.length) {
        orderedDashboards.value = orderedByGroup(
            settingsStore.menuOrder,
            'dashboards',
            [...settingsStore.dashboards],
            (dashboard) => dashboard.uid,
        )
        return
    }
    orderedDashboards.value = [...settingsStore.dashboards].sort((a, b) => {
        if (a.uid === settingsStore.homeDashboard) return -1
        if (b.uid === settingsStore.homeDashboard) return 1
        return 0
    })
})
const persistDashboardOrder = (): void => {
    settingsStore.menuOrder = setGroupOrder(
        settingsStore.menuOrder,
        'dashboards',
        orderedDashboards.value.map((dashboard) => dashboard.uid),
    )
}

// Shared with the search palette so the two cannot drift.
const { addDashboard } = useDashboardActions()

const orderedAlerts = ref<typeof settingsStore.alerts>([])
watchEffect(() => {
    orderedAlerts.value = orderedByGroup(
        settingsStore.menuOrder,
        'alerts',
        [...settingsStore.alerts],
        (alert) => alert.uid,
    )
})
const persistAlertOrder = (): void => {
    settingsStore.menuOrder = setGroupOrder(
        settingsStore.menuOrder,
        'alerts',
        orderedAlerts.value.map((alert) => alert.uid),
    )
}
// The same set the header bell uses, so an expiring silence updates both.
const activeAlertCount = computed(() => settingsStore.alertsNeedingAttention.length)
// Menu glyph: shape encodes silenced/disabled, color keeps the live state
// (silenced alerts still evaluate; a red sleep bell means firing-but-muted).
const alertMenuIcon = (alert: Alert): string => {
    if (!alert.enabled) return mdiBellOffOutline
    if (alertIsSilenced(alert)) return mdiBellSleepOutline
    return getAlertStateIcon(alert.state)
}
const alertMenuIconClass = (alert: Alert): string =>
    alert.enabled ? getAlertStateClass(alert.state) : 'text-text-color-secondary'
// Keeps a row's hover actions visible while its tag popover is open.
const openTagRow = ref<string | null>(null)
const onTagOpen = (rowKey: string, open: boolean): void => {
    openTagRow.value = open ? rowKey : null
}

// Pinned rows are shortcuts, so they go to the channel's canonical page.
const sensorRoute = (sensor: MonitoringSensor) =>
    channelRoute(deviceStore.allDevices(), sensor.deviceUID, sensor.channelName)

// The section's own listing keeps fans on their Monitoring chart instead.
const listedSensorRoute = (sensor: MonitoringSensor) =>
    monitoringChannelRoute(deviceStore.allDevices(), sensor.deviceUID, sensor.channelName)

// The row wrapper needs the same target its link uses, so both read it here.
const dashboardTarget = (dashboardUID: UID): RouteLocationRaw => ({
    name: 'monitoring-dashboard',
    params: { dashboardUID },
})
const pinnedTarget = (row: PinnedRow): RouteLocationRaw =>
    row.kind === 'dashboard' ? dashboardTarget(row.dashboard.uid) : sensorRoute(row.sensor)
const isRouteActive = useRouteActive()
</script>

<template>
    <div class="flex flex-col gap-0.5 p-2 pb-24 text-base">
        <template v-if="pinnedRows.length > 0">
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
                    :class="{ 'bg-surface-hover': isRouteActive(pinnedTarget(row)) }"
                >
                    <template v-if="row.kind === 'dashboard'">
                        <RouterLink
                            :to="dashboardTarget(row.dashboard.uid)"
                            class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none"
                            exact-active-class="!text-accent"
                        >
                            <svg-icon
                                type="mdi"
                                :path="mdiViewDashboardOutline"
                                :size="18"
                                class="shrink-0 text-text-color-secondary"
                            />
                            <span class="truncate">{{ row.dashboard.name }}</span>
                        </RouterLink>
                        <div
                            class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex group-has-[[data-state=open]]:flex"
                        >
                            <button
                                type="button"
                                class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                v-tooltip.top="t('layout.shell.coolingPanel.unpin')"
                                @click.prevent="toggleDashboardPin(row.dashboard)"
                            >
                                <svg-icon type="mdi" :path="mdiPinOff" :size="16" />
                            </button>
                            <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                                <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                            </span>
                        </div>
                    </template>
                    <template v-else>
                        <RouterLink
                            :to="sensorRoute(row.sensor)"
                            class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none"
                            exact-active-class="!text-accent"
                        >
                            <svg-icon
                                type="mdi"
                                :path="sensorIcon(row.sensor)"
                                :size="18"
                                class="shrink-0"
                                :class="{ 'animate-spin-slow': sensorSpins(row.sensor) }"
                                :style="{
                                    color:
                                        sensorColor(row.sensor.deviceUID, row.sensor.channelName) ||
                                        undefined,
                                }"
                            />
                            <span class="truncate">
                                {{ sensorLabel(row.sensor.deviceUID, row.sensor.channelName) }}
                            </span>
                            <TagChips
                                :device-u-i-d="row.sensor.deviceUID"
                                :channel-name="row.sensor.channelName"
                            />
                            <!-- Huge shrink so the device name fully collapses before
                                 the channel name (truncate, shrink-1) gives up any space. -->
                            <span class="shrink-[9999] truncate text-xs text-text-color-secondary">
                                {{ deviceLabel(row.sensor.deviceUID) }}
                            </span>
                            <span
                                class="ml-auto whitespace-nowrap font-numeric tabular-nums text-text-color group-hover:hidden group-has-[:focus-visible]:hidden"
                                :class="{
                                    '!hidden':
                                        openTagRow ===
                                        `pin-${row.sensor.deviceUID}-${row.sensor.channelName}`,
                                }"
                            >
                                {{ liveValue(row.sensor) }}
                            </span>
                        </RouterLink>
                        <div
                            class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex group-has-[[data-state=open]]:flex"
                            :class="{
                                '!flex':
                                    openTagRow ===
                                    `pin-${row.sensor.deviceUID}-${row.sensor.channelName}`,
                            }"
                        >
                            <button
                                v-if="alertKind(row.sensor) != null"
                                type="button"
                                class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                v-tooltip.top="
                                    alertKind(row.sensor) === 'fan'
                                        ? t('layout.shell.monitoringPanel.failAlert')
                                        : t('layout.shell.monitoringPanel.createAlert')
                                "
                                @click.prevent="createAlert(row.sensor)"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="
                                        alertKind(row.sensor) === 'fan'
                                            ? mdiFanAlert
                                            : mdiBellPlusOutline
                                    "
                                    :size="16"
                                />
                            </button>
                            <TagPopover
                                :device-u-i-d="row.sensor.deviceUID"
                                :channel-name="row.sensor.channelName"
                                @open="
                                    (open: boolean) =>
                                        onTagOpen(
                                            `pin-${row.sensor.deviceUID}-${row.sensor.channelName}`,
                                            open,
                                        )
                                "
                            />
                            <span class="flex w-6 shrink-0 justify-center">
                                <CCColorPicker
                                    :model-value="
                                        sensorColor(row.sensor.deviceUID, row.sensor.channelName)
                                    "
                                    :default-color="
                                        sensorDefaultColor(
                                            row.sensor.deviceUID,
                                            row.sensor.channelName,
                                        )
                                    "
                                    :size="1.25"
                                    @update:model-value="
                                        (c: Color) => setSensorColor(row.sensor, c)
                                    "
                                    @reset="resetSensorColor(row.sensor)"
                                />
                            </span>
                            <button
                                type="button"
                                class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                v-tooltip.top="t('layout.shell.coolingPanel.unpin')"
                                @click.prevent="togglePin(row.sensor)"
                            >
                                <svg-icon type="mdi" :path="mdiPinOff" :size="16" />
                            </button>
                            <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                                <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                            </span>
                        </div>
                    </template>
                </div>
            </VueDraggable>
            <UiSeparator class="my-1" />
        </template>

        <PanelHeader :label="t('layout.menu.dashboards')">
            <button
                type="button"
                class="rounded p-0.5 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                v-tooltip.top="t('layout.menu.tooltips.addDashboard')"
                @click="addDashboard"
            >
                <svg-icon type="mdi" :path="mdiPlus" :size="16" />
            </button>
        </PanelHeader>
        <VueDraggable
            :force-auto-scroll-fallback="true"
            v-model="orderedDashboards"
            handle=".drag-handle"
            :animation="150"
            class="flex flex-col gap-0.5"
            @end="persistDashboardOrder"
        >
            <div
                v-for="dashboard in orderedDashboards"
                :key="dashboard.uid"
                class="group flex items-center rounded-lg hover:bg-surface-hover has-[:focus-visible]:bg-surface-hover has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent"
                :class="{ 'bg-surface-hover': isRouteActive(dashboardTarget(dashboard.uid)) }"
            >
                <RouterLink
                    :to="dashboardTarget(dashboard.uid)"
                    class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none"
                    exact-active-class="!text-accent"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiViewDashboardOutline"
                        :size="18"
                        class="shrink-0 text-text-color-secondary"
                    />
                    <span class="truncate">{{ dashboard.name }}</span>
                    <svg-icon
                        v-if="dashboard.uid === settingsStore.homeDashboard"
                        type="mdi"
                        :path="mdiHome"
                        :size="14"
                        class="shrink-0 text-text-color-secondary"
                        v-tooltip.top="t('views.dashboard.setAsHome')"
                    />
                </RouterLink>
                <div
                    class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex group-has-[[data-state=open]]:flex"
                >
                    <button
                        type="button"
                        class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                        v-tooltip.top="
                            isDashboardPinned(dashboard)
                                ? t('layout.shell.coolingPanel.unpin')
                                : t('layout.shell.coolingPanel.pin')
                        "
                        @click.prevent="toggleDashboardPin(dashboard)"
                    >
                        <svg-icon
                            type="mdi"
                            :path="isDashboardPinned(dashboard) ? mdiPinOff : mdiPinOutline"
                            :size="16"
                        />
                    </button>
                    <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                        <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                    </span>
                </div>
            </div>
        </VueDraggable>
        <UiSeparator class="my-1" />
        <PanelHeader>
            <template #label>
                {{ t('layout.menu.alerts') }}
                <span
                    v-if="activeAlertCount > 0"
                    class="rounded-full bg-error px-1.5 text-xs normal-case text-error-fg"
                >
                    {{ activeAlertCount }}
                </span>
            </template>
            <button
                type="button"
                class="rounded p-0.5 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                v-tooltip.top="t('layout.menu.tooltips.addAlert')"
                @click="router.push({ name: 'monitoring-alert-new' })"
            >
                <svg-icon type="mdi" :path="mdiPlus" :size="16" />
            </button>
        </PanelHeader>
        <RouterLink
            :to="{ name: 'monitoring-alerts' }"
            class="flex items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
            exact-active-class="bg-surface-hover !text-accent"
        >
            <svg-icon
                type="mdi"
                :path="mdiBellOutline"
                :size="18"
                class="shrink-0 text-text-color-secondary"
            />
            <span class="truncate">{{ t('views.alerts.alertsOverview') }}</span>
        </RouterLink>
        <VueDraggable
            :force-auto-scroll-fallback="true"
            v-model="orderedAlerts"
            handle=".drag-handle"
            :animation="150"
            class="flex flex-col gap-0.5"
            @end="persistAlertOrder"
        >
            <RouterLink
                v-for="alert in orderedAlerts"
                :key="alert.uid"
                :to="{ name: 'monitoring-alert', params: { alertUID: alert.uid } }"
                class="group flex items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                exact-active-class="bg-surface-hover !text-accent"
            >
                <svg-icon
                    type="mdi"
                    :path="alertMenuIcon(alert)"
                    :size="18"
                    class="shrink-0"
                    :class="alertMenuIconClass(alert)"
                />
                <span class="truncate">{{ alert.name }}</span>
                <span
                    class="ml-auto hidden items-center gap-0.5 group-hover:inline-flex group-has-[[data-state=open]]:inline-flex"
                    @click.prevent.stop
                >
                    <AlertSilenceMenu v-if="alert.enabled" :alert="alert">
                        <template #trigger>
                            <button
                                type="button"
                                class="rounded p-0.5 text-text-color-secondary hover:text-text-color"
                                v-tooltip.top="t('views.alerts.silenceTooltip')"
                            >
                                <svg-icon type="mdi" :path="mdiBellSleepOutline" :size="16" />
                            </button>
                        </template>
                    </AlertSilenceMenu>
                    <button
                        type="button"
                        class="rounded p-0.5 text-text-color-secondary hover:text-text-color"
                        v-tooltip.top="
                            alert.enabled
                                ? t('views.alerts.disableAlert')
                                : t('views.alerts.enableAlert')
                        "
                        @click="settingsStore.setAlertEnabled(alert.uid, !alert.enabled)"
                    >
                        <svg-icon
                            type="mdi"
                            :path="alert.enabled ? mdiBellOffOutline : mdiBellOutline"
                            :size="16"
                        />
                    </button>
                </span>
                <span
                    class="drag-handle hidden cursor-grab p-0.5 text-text-color-secondary group-hover:inline-flex"
                >
                    <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                </span>
            </RouterLink>
        </VueDraggable>
        <UiSeparator class="my-1" />
        <VueDraggable
            :force-auto-scroll-fallback="true"
            v-model="groups"
            handle=".device-drag-handle"
            :animation="150"
            class="flex flex-col gap-0.5"
            @end="persistDeviceOrder"
        >
            <div v-for="group in groups" :key="group.deviceUID" class="flex flex-col gap-0.5">
                <!-- The whole band is the link, as a channel row is, so the active
                     device reads the same way here as a selected channel does. The
                     label keeps the device color rather than turning accent: the
                     color is what identifies the device across every panel. -->
                <PanelHeader
                    class="group/device rounded-t-lg hover:bg-surface-hover"
                    :class="{
                        'bg-surface-hover': isRouteActive(deviceTarget(group.deviceUID)),
                    }"
                    :to="deviceTarget(group.deviceUID)"
                    :color="deviceColor(group.deviceUID) || 'rgb(var(--colors-text-color))'"
                >
                    <template #label>{{ deviceLabel(group.deviceUID) }}</template>
                    <!-- invisible, not hidden: the header must not change height on
                         hover. -mr-1 cancels the header's pr-2 down to the pr-1 the
                         rows below inset by, so the same p-1 handle puts the drag
                         glyph on the same column. -->
                    <span
                        class="invisible -mr-1 flex items-center gap-0.5 group-hover/device:visible group-has-[:focus-visible]/device:visible"
                        :class="{ '!visible': openColorDevice === group.deviceUID }"
                    >
                        <span class="flex w-6 shrink-0 justify-center">
                            <CCColorPicker
                                :model-value="devicePickerColor(group.deviceUID)"
                                :size="1.25"
                                @open="
                                    (open: boolean) =>
                                        (openColorDevice = open ? group.deviceUID : undefined)
                                "
                                @update:model-value="
                                    (c: Color) => setDeviceColor(group.deviceUID, c)
                                "
                            />
                        </span>
                        <!-- The rows' p-1 grab area, with -my-0.5 giving the extra
                             height back so the header does not grow for it. -->
                        <span
                            class="device-drag-handle -my-0.5 cursor-grab p-1 text-text-color-secondary"
                        >
                            <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                        </span>
                    </span>
                </PanelHeader>
                <VueDraggable
                    :force-auto-scroll-fallback="true"
                    v-model="group.sensors"
                    handle=".drag-handle"
                    :animation="150"
                    class="flex flex-col gap-0.5"
                    @end="persistSensorOrder(group)"
                >
                    <div
                        v-for="sensor in group.sensors"
                        :key="sensor.channelName"
                        class="group flex items-center rounded-lg hover:bg-surface-hover has-[:focus-visible]:bg-surface-hover has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent"
                        :class="{ 'bg-surface-hover': isRouteActive(listedSensorRoute(sensor)) }"
                    >
                        <RouterLink
                            :to="listedSensorRoute(sensor)"
                            class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none"
                            exact-active-class="!text-accent"
                        >
                            <svg-icon
                                type="mdi"
                                :path="sensorIcon(sensor)"
                                :size="18"
                                class="shrink-0"
                                :class="{ 'animate-spin-slow': sensorSpins(sensor) }"
                                :style="{
                                    color:
                                        sensorColor(sensor.deviceUID, sensor.channelName) ||
                                        undefined,
                                }"
                            />
                            <span class="truncate">
                                {{ sensorLabel(sensor.deviceUID, sensor.channelName) }}
                            </span>
                            <TagChips
                                :device-u-i-d="sensor.deviceUID"
                                :channel-name="sensor.channelName"
                            />
                            <UiTooltip
                                v-if="isUnhealthy(sensor.deviceUID, sensor.channelName)"
                                :text="healthTooltip(sensor.deviceUID, sensor.channelName)"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="mdiAlert"
                                    :size="14"
                                    class="shrink-0 text-error"
                                />
                            </UiTooltip>
                            <span
                                class="ml-auto whitespace-nowrap font-numeric tabular-nums text-text-color group-hover:hidden group-has-[:focus-visible]:hidden"
                                :class="{
                                    '!hidden':
                                        openTagRow === `${sensor.deviceUID}-${sensor.channelName}`,
                                }"
                            >
                                {{ liveValue(sensor) }}
                            </span>
                        </RouterLink>
                        <div
                            class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex group-has-[[data-state=open]]:flex"
                            :class="{
                                '!flex': openTagRow === `${sensor.deviceUID}-${sensor.channelName}`,
                            }"
                        >
                            <button
                                v-if="alertKind(sensor) != null"
                                type="button"
                                class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                v-tooltip.top="
                                    alertKind(sensor) === 'fan'
                                        ? t('layout.shell.monitoringPanel.failAlert')
                                        : t('layout.shell.monitoringPanel.createAlert')
                                "
                                @click.prevent="createAlert(sensor)"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="
                                        alertKind(sensor) === 'fan'
                                            ? mdiFanAlert
                                            : mdiBellPlusOutline
                                    "
                                    :size="16"
                                />
                            </button>
                            <TagPopover
                                :device-u-i-d="sensor.deviceUID"
                                :channel-name="sensor.channelName"
                                @open="
                                    (open: boolean) =>
                                        onTagOpen(`${sensor.deviceUID}-${sensor.channelName}`, open)
                                "
                            />
                            <span class="flex w-6 shrink-0 justify-center">
                                <CCColorPicker
                                    :model-value="sensorColor(sensor.deviceUID, sensor.channelName)"
                                    :default-color="
                                        sensorDefaultColor(sensor.deviceUID, sensor.channelName)
                                    "
                                    :size="1.25"
                                    @update:model-value="(c: Color) => setSensorColor(sensor, c)"
                                    @reset="resetSensorColor(sensor)"
                                />
                            </span>
                            <button
                                type="button"
                                class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                v-tooltip.top="
                                    isPinned(sensor)
                                        ? t('layout.shell.coolingPanel.unpin')
                                        : t('layout.shell.coolingPanel.pin')
                                "
                                @click.prevent="togglePin(sensor)"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="isPinned(sensor) ? mdiPinOff : mdiPinOutline"
                                    :size="16"
                                />
                            </button>
                            <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                                <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                            </span>
                        </div>
                    </div>
                </VueDraggable>
            </div>
        </VueDraggable>
    </div>
</template>
