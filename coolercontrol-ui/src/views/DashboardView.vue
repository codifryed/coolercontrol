<!--
  SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { useSettingsStore } from '@/stores/SettingsStore'
import { computed, nextTick, onMounted, onUnmounted, type Ref, ref, watch } from 'vue'
import { type Color, DeviceType, type UID } from '@/models/Device.ts'
import {
    ChartType,
    Dashboard,
    DashboardDeviceChannel,
    DataType,
    getLocalizedChartType,
    getLocalizedDataType,
} from '@/models/Dashboard.ts'
import { $enum } from 'ts-enum-util'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import AxisOptions from '@/components/AxisOptions.vue'
import { TempInfo } from '@/models/TempInfo.ts'
import { ChannelInfo } from '@/models/ChannelInfo.ts'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiAlertOutline,
    mdiContentCopy,
    mdiFan,
    mdiHome,
    mdiHomeOutline,
    mdiOverscan,
    mdiRestart,
    mdiThermometer,
    mdiTrashCanOutline,
} from '@mdi/js'
import SensorTable from '@/components/SensorTable.vue'
import TimeChart from '@/components/TimeChart.vue'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'
import { component as Fullscreen } from 'vue-fullscreen'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { useWindowSize } from '@vueuse/core'
import { useConfirm } from '@/shell/confirm'
import UiButton from '@/shell/ui/UiButton.vue'
import UiMultiSelect from '@/shell/ui/UiMultiSelect.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiSelect from '@/shell/ui/UiSelect.vue'
import EntityTitleRename from '@/components/EntityTitleRename.vue'
import EntityPageHeader from '@/components/EntityPageHeader.vue'
import HealthWarning from '@/components/HealthWarning.vue'
import HelpIcon from '@/components/info/HelpIcon.vue'

interface Props {
    dashboardUID?: UID
    // When both are set, the view renders the channel's private quick-chart
    // dashboard (channelDashboard) with the filter controls hidden.
    deviceUID?: UID
    channelName?: string
}

const props = defineProps<Props>()
const { t } = useI18n()
const router = useRouter()
const confirm = useConfirm()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const sensorMode: boolean = props.deviceUID != null && props.channelName != null

const coolingLabel = computed(() => t('layout.shell.cooling'))

// Entity-first companion link: fan/pump channels also have a Cooling page.
const hasCoolingPage = computed((): boolean => {
    if (!sensorMode) return false
    for (const device of deviceStore.allDevices()) {
        if (device.uid === props.deviceUID) {
            return device.info?.channels.get(props.channelName!)?.speed_options != null
        }
    }
    return false
})

// Custom sensors are edited under Devices.
const isCustomSensor = computed((): boolean => {
    if (!sensorMode) return false
    for (const device of deviceStore.allDevices()) {
        if (device.uid === props.deviceUID) return device.type === DeviceType.CUSTOM_SENSORS
    }
    return false
})

const channelLabel = ref(
    sensorMode
        ? (settingsStore.allUIDeviceSettings
              .get(props.deviceUID!)
              ?.sensorsAndChannels.get(props.channelName!)?.name ?? props.channelName!)
        : '',
)
const createChannelDashboard = (): Dashboard => {
    const dash = new Dashboard(channelLabel.value)
    dash.timeRangeSeconds = 300
    // needed due to reduced default data type range:
    dash.dataTypes = []
    dash.deviceChannelNames.push(new DashboardDeviceChannel(props.deviceUID!, props.channelName!))
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID!)!
        .sensorsAndChannels.get(props.channelName!)!.channelDashboard = dash
    return dash
}

// Sensors fall back to their detected label; a dashboard keeps its own name.
const fallbackName = computed(() =>
    sensorMode
        ? settingsStore.defaultChannelLabel(props.deviceUID!, props.channelName!)
        : dashboard.name,
)

const saveNameFunction = async (newName: string): Promise<boolean> => {
    if (sensorMode) {
        // User names are persisted as daemon name overrides. An empty name
        // removes the override and falls back to the detected label, which has
        // to be read before saving drops the override it is derived from.
        const applied = newName.length > 0 ? newName : fallbackName.value
        const success = await settingsStore.saveChannelName(
            props.deviceUID!,
            props.channelName!,
            newName,
        )
        if (!success) return false
        channelLabel.value = applied
        return true
    }
    if (newName.length > 0) {
        dashboard.name = newName
    }
    return false
}

const homeDashboard: Dashboard =
    settingsStore.dashboards.find((dashboard) => dashboard.uid === settingsStore.homeDashboard) ??
    settingsStore.dashboards[0] // show first dashboard if no Home Dashboard set
const dashboard: Dashboard = sensorMode
    ? (settingsStore.allUIDeviceSettings
          .get(props.deviceUID!)!
          .sensorsAndChannels.get(props.channelName!)!.channelDashboard ?? createChannelDashboard())
    : props.dashboardUID != null
      ? (settingsStore.dashboards.find((d) => d.uid === props.dashboardUID) ?? homeDashboard)
      : homeDashboard

// Migrate removed Controls chart type to Time Chart
if ((dashboard.chartType as string) === 'Controls') {
    dashboard.chartType = ChartType.TIME_CHART
}
// A saved types filter on a channel dashboard would annoyingly hide some
// metrics like i.e. RPMs.
if (sensorMode && dashboard.dataTypes.length > 0) {
    dashboard.dataTypes = []
}

const isHome = computed(() => settingsStore.homeDashboard === dashboard.uid)
const setHome = (): void => {
    settingsStore.homeDashboard = dashboard.uid
}

// Mobile has no side panel, so the dashboard toolbar carries the dashboard switcher
// (plus an Alerts entry) that the Monitoring panel provides on desktop.
const { width } = useWindowSize()
const isMobile = computed(() => width.value < 768)
const ALERTS_NAV = '__alerts__'
const dashboardNavOptions = computed(() => [
    ...settingsStore.dashboards.map((d) => ({ label: d.name, value: d.uid })),
    { label: t('layout.menu.alerts'), value: ALERTS_NAV },
])
const dashboardNav = computed<string>({
    get: () => dashboard.uid,
    set: (value) => {
        if (value === ALERTS_NAV) {
            router.push({ name: 'monitoring-alerts' })
        } else if (value !== dashboard.uid) {
            router.push({ name: 'monitoring-dashboard', params: { dashboardUID: value } })
        }
    },
})
const duplicateDashboard = (): void => {
    const copy = new Dashboard(`${dashboard.name} (copy)`)
    copy.chartType = dashboard.chartType
    copy.timeRangeSeconds = dashboard.timeRangeSeconds
    copy.autoScaleDegree = dashboard.autoScaleDegree
    copy.autoScaleFrequency = dashboard.autoScaleFrequency
    copy.autoScaleWatts = dashboard.autoScaleWatts
    copy.degreeMax = dashboard.degreeMax
    copy.degreeMin = dashboard.degreeMin
    copy.frequencyMax = dashboard.frequencyMax
    copy.frequencyMin = dashboard.frequencyMin
    copy.wattsMax = dashboard.wattsMax
    copy.wattsMin = dashboard.wattsMin
    copy.dataTypes = [...dashboard.dataTypes]
    copy.selectedTags = [...dashboard.selectedTags]
    copy.deviceChannelNames = dashboard.deviceChannelNames.map(
        (ch) => new DashboardDeviceChannel(ch.deviceUID, ch.channelName),
    )
    settingsStore.dashboards.push(copy)
    router.push({ name: 'monitoring-dashboard', params: { dashboardUID: copy.uid } })
}
const deleteDashboard = (): void => {
    confirm.require({
        message: t('views.dashboard.deleteDashboardConfirm', { name: dashboard.name }),
        header: t('views.dashboard.deleteDashboard'),
        icon: mdiAlertOutline,
        accept: () => {
            const index = settingsStore.dashboards.findIndex((d) => d.uid === dashboard.uid)
            if (index === -1) return
            settingsStore.dashboards.splice(index, 1)
            if (settingsStore.homeDashboard === dashboard.uid) {
                settingsStore.homeDashboard = undefined
            }
            router.push({ name: 'section-monitoring' })
        },
    })
}

const chartTypes = [...$enum(ChartType).values()].map((type) => ({
    value: type,
    text: getLocalizedChartType(type),
}))

const chartMinutesMin: number = 1
const chartMinutesMax: number = 60
const chartMinutes: Ref<number> = ref(dashboard.timeRangeSeconds / 60)
const chartMinutesChanged = (value: number): void => {
    dashboard.timeRangeSeconds = value * 60
}
const chartMinutesScrolled = (event: WheelEvent): void => {
    if (event.deltaY < 0) {
        if (chartMinutes.value < chartMinutesMax) chartMinutes.value += 1
    } else {
        if (chartMinutes.value > chartMinutesMin) chartMinutes.value -= 1
    }
}

const dataTypes = [...$enum(DataType).values()].map((type) => ({
    value: type,
    text: getLocalizedDataType(type),
}))

const fullPage = ref(false)

interface AvailableSensor {
    name: string
    deviceUID: UID // This is needed for the dropdown selector (it only holds children)
    label: string
    color: Color
}

interface AvailableSensorSource {
    deviceUID: UID
    deviceName: string
    sensors: Array<AvailableSensor>
}

const chosenSensorSources: Ref<Array<AvailableSensor>> = ref([])
const sensorSources: Ref<Array<AvailableSensorSource>> = ref([])
const fillSensorSources = (): void => {
    sensorSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.info == null) continue
        if (device.info.channels.size === 0 && device.info.temps.size === 0) {
            continue
        }
        const sensors: Array<AvailableSensor> = []
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        device.info.temps.forEach((_: TempInfo, key: string) => {
            const sensorSettings = deviceSettings.sensorsAndChannels.get(key)!
            sensors.push({
                name: key,
                deviceUID: device.uid,
                label: sensorSettings.name,
                color: sensorSettings.color,
            })
        })
        device.info.channels.forEach((value: ChannelInfo, key: string) => {
            if (value.lcd_modes.length > 0 || value.lighting_modes.length > 0) return
            const sensorSettings = deviceSettings.sensorsAndChannels.get(key)!
            sensors.push({
                name: key,
                deviceUID: device.uid,
                label: sensorSettings.name,
                color: sensorSettings.color,
            })
        })
        sensorSources.value.push({
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            sensors: sensors,
        })
    }
}
fillSensorSources()
const fillChosenSensorSources = (): void => {
    chosenSensorSources.value.length = 0
    const deviceChannelsMap: Map<UID, Array<string>> = new Map()
    dashboard.deviceChannelNames.forEach((deviceChannel: DashboardDeviceChannel) => {
        if (deviceChannelsMap.has(deviceChannel.deviceUID)) {
            deviceChannelsMap.get(deviceChannel.deviceUID)!.push(deviceChannel.channelName)
        } else {
            deviceChannelsMap.set(deviceChannel.deviceUID, [deviceChannel.channelName])
        }
    })
    sensorSources.value.forEach((sensorSource: AvailableSensorSource) => {
        if (!deviceChannelsMap.has(sensorSource.deviceUID)) return
        const sensorsToAdd: Array<AvailableSensor> = []
        deviceChannelsMap.get(sensorSource.deviceUID)!.forEach((channelName: string) => {
            sensorSource.sensors.forEach((availableSensor: AvailableSensor) => {
                if (availableSensor.name !== channelName) return
                sensorsToAdd.push({
                    name: availableSensor.name,
                    deviceUID: availableSensor.deviceUID,
                    label: availableSensor.label,
                    color: availableSensor.color,
                })
            })
        })
        chosenSensorSources.value.push(...sensorsToAdd)
    })
}
fillChosenSensorSources()

const sensorKey = (sensor: AvailableSensor): string => `${sensor.deviceUID}/${sensor.name}`
const sensorGroups = computed(() =>
    filteredSensorSources.value.map((source) => ({
        label: source.deviceName,
        options: source.sensors.map((sensor) => ({
            label: sensor.label,
            value: sensorKey(sensor),
            color: sensor.color,
        })),
    })),
)
const chosenSensorKeys = computed<string[]>({
    get: () => chosenSensorSources.value.map(sensorKey),
    set: (keys) => {
        const all = sensorSources.value.flatMap((source) => source.sensors)
        chosenSensorSources.value = keys
            .map((key) => all.find((sensor) => sensorKey(sensor) === key))
            .filter((sensor): sensor is AvailableSensor => sensor != null)
        updateDashboardSensorsFilter(chosenSensorSources.value)
    },
})
const tagGroups = computed(() => [
    {
        label: '',
        options: tagOptions.value.map((tag) => ({
            label: tag.name,
            value: tag.name,
            color: tag.color,
            dot: true,
        })),
    },
])
const dataTypeGroups = computed(() => [
    { label: '', options: dataTypes.map((entry) => ({ label: entry.text, value: entry.value })) },
])
const chartTypeOptions = chartTypes.map((entry) => ({ label: entry.text, value: entry.value }))

const updateDashboardSensorsFilter = (sensorSources: Array<AvailableSensor>): void => {
    const newSensorsFilter: Array<DashboardDeviceChannel> = []
    sensorSources.forEach((sensor: AvailableSensor) => {
        newSensorsFilter.push(new DashboardDeviceChannel(sensor.deviceUID, sensor.name))
    })
    dashboard.deviceChannelNames = newSensorsFilter
}

// Tag filter options for MultiSelect
interface TagOption {
    name: string
    color: string
}
const tagOptions = computed<Array<TagOption>>(() =>
    Array.from(settingsStore.tags.entries()).map(([name, tag]) => ({
        name,
        color: tag.color,
    })),
)

// Channels resolved from selected tags (computed reactively)
const tagResolvedChannels = computed<Array<DashboardDeviceChannel>>(() => {
    const channels: Array<DashboardDeviceChannel> = []
    for (const tagName of dashboard.selectedTags) {
        for (const { deviceUID, channelName } of settingsStore.getTagChannels(tagName)) {
            if (!channels.some((c) => c.deviceUID === deviceUID && c.channelName === channelName)) {
                channels.push(new DashboardDeviceChannel(deviceUID, channelName))
            }
        }
    }
    return channels
})

// Effective channels = tag channels union manual channels
const effectiveChannels = computed<Array<DashboardDeviceChannel>>(() => {
    const merged = [...tagResolvedChannels.value]
    for (const ch of dashboard.deviceChannelNames) {
        if (!merged.some((m) => m.deviceUID === ch.deviceUID && m.channelName === ch.channelName)) {
            merged.push(ch)
        }
    }
    return merged
})

// Filter channel MultiSelect options to exclude tag-covered channels
const filteredSensorSources = computed<Array<AvailableSensorSource>>(() => {
    if (tagResolvedChannels.value.length === 0) return sensorSources.value
    return sensorSources.value
        .map((source) => ({
            ...source,
            sensors: source.sensors.filter(
                (s) =>
                    !tagResolvedChannels.value.some(
                        (tc) => tc.deviceUID === s.deviceUID && tc.channelName === s.name,
                    ),
            ),
        }))
        .filter((source) => source.sensors.length > 0)
})

// Remove stale tags that no longer exist from the dashboard's selection
const cleanupStaleTags = (): void => {
    const validTags = new Set(settingsStore.tags.keys())
    const remaining = dashboard.selectedTags.filter((t) => validTags.has(t))
    // Only reassign when something was actually removed, otherwise we mutate the
    // dashboard on every navigation and trigger an unnecessary settings save.
    if (remaining.length !== dashboard.selectedTags.length) {
        dashboard.selectedTags = remaining
    }
}
cleanupStaleTags()

// When tags change, remove tag-covered channels from manual selection for clarity
watch(
    () => dashboard.selectedTags,
    () => {
        if (tagResolvedChannels.value.length === 0) return
        // Remove from manual channel list any channels now covered by tags
        dashboard.deviceChannelNames = dashboard.deviceChannelNames.filter(
            (ch) =>
                !tagResolvedChannels.value.some(
                    (tc) => tc.deviceUID === ch.deviceUID && tc.channelName === ch.channelName,
                ),
        )
        fillChosenSensorSources()
    },
    { deep: true },
)

// Dashboard view with effective channels (tag union manual) for chart/table components
const viewDashboard = computed(() => ({
    ...dashboard,
    deviceChannelNames: effectiveChannels.value,
}))

const addScrollEventListener = (): void => {
    // @ts-ignore
    document?.querySelector('.chart-minutes')?.addEventListener('wheel', chartMinutesScrolled)
}
const updateResponsiveGraphHeight = (): void => {
    const graphEl = document.getElementById('u-plot-chart')
    if (graphEl != null) {
        if (fullPage.value) {
            graphEl.style.height = 'calc(100vh - 1rem)'
            return
        }
        // Fill the viewport from wherever the chart starts; works both at the page
        // top and inside the shell content area. On mobile the bottom nav sits at the
        // viewport foot, so subtract its height or the chart runs under it.
        const top = Math.ceil(graphEl.getBoundingClientRect().top)
        const bottomNav = document.getElementById('shell-bottom-nav')
        const bottomNavHeight = bottomNav ? Math.ceil(bottomNav.getBoundingClientRect().height) : 0
        graphEl.style.height = `calc(100vh - ${top + 12 + bottomNavHeight}px)`
    }
}

const toggleFullPage = async (): Promise<void> => {
    fullPage.value = !fullPage.value
    // The chart is teleported in and out of the full-page wrapper, so measuring
    // it before the re-render reads its old position: leaving full page would
    // otherwise keep a viewport-tall chart.
    await nextTick()
    updateResponsiveGraphHeight()
}

const chartKey: Ref<string> = ref(uuidV4())
const sensorTableRef = ref<InstanceType<typeof SensorTable> | null>(null)
let panelResizeObserver: ResizeObserver | null = null
onMounted(async () => {
    window.addEventListener('resize', updateResponsiveGraphHeight)
    setTimeout(updateResponsiveGraphHeight)
    // Re-fit graph height when the control panel reflows (e.g. filter chips wrap to a new row).
    const controlPanel = document.getElementById('control-panel')
    if (controlPanel != null) {
        panelResizeObserver = new ResizeObserver(() => updateResponsiveGraphHeight())
        panelResizeObserver.observe(controlPanel)
    }

    addScrollEventListener()
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
    })
    // chartKey regenerating remounts TimeChart, replacing the #u-plot-chart node and
    // losing the inline height. Re-apply it once the new node is in the DOM.
    watch(chartKey, async () => {
        await nextTick()
        updateResponsiveGraphHeight()
    })
    // This forces a debounced chart redraw for any dashboard settings change:
    watch(
        [settingsStore.dashboards, settingsStore.allUIDeviceSettings],
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
    )
    watch(
        settingsStore.allDaemonDeviceSettings,
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
    )
})
onUnmounted(() => {
    window.removeEventListener('resize', updateResponsiveGraphHeight)
    panelResizeObserver?.disconnect()
    panelResizeObserver = null
})
</script>

<template>
    <div class="flex h-full flex-col">
        <entity-page-header id="control-panel">
            <template #title>
                <entity-title-rename
                    v-if="sensorMode || !isMobile"
                    :current-name="sensorMode ? channelLabel : dashboard.name"
                    :fallback-name="fallbackName"
                    :save-name-function="saveNameFunction"
                />
                <span v-else class="w-full p-2">
                    <UiSelect
                        v-model="dashboardNav"
                        :options="dashboardNavOptions"
                        class="w-full"
                    />
                </span>
                <HelpIcon
                    v-if="dashboard.chartType == ChartType.TIME_CHART"
                    class="ml-1"
                    :text="t('views.dashboard.mouseActions')"
                    side="bottom"
                />
            </template>
            <template #controls>
                <UiButton
                    v-if="hasCoolingPage"
                    variant="outline"
                    v-tooltip.top="t('views.dashboard.openCooling')"
                    @click="
                        router.push({
                            name: 'cooling-channel',
                            params: {
                                deviceUID: props.deviceUID!,
                                channelName: props.channelName!,
                            },
                        })
                    "
                >
                    <svg-icon type="mdi" :path="mdiFan" :size="deviceStore.getREMSize(1.1)" />
                    <span class="ml-1">{{ coolingLabel }}</span>
                </UiButton>
                <div
                    v-if="!sensorMode && settingsStore.tags.size > 0"
                    class="p-2 pr-0 flex flex-row"
                >
                    <span v-tooltip.top="t('views.dashboard.filterByTag')">
                        <UiMultiSelect
                            v-model="dashboard.selectedTags"
                            :groups="tagGroups"
                            class="w-36"
                            :placeholder="t('views.dashboard.filterTags')"
                        />
                    </span>
                </div>
                <div v-if="!sensorMode" class="p-2 pr-0 flex flex-row">
                    <span v-tooltip.top="t('views.dashboard.filterBySensor')">
                        <UiMultiSelect
                            v-model="chosenSensorKeys"
                            :groups="sensorGroups"
                            class="w-36"
                            filter
                            :filter-placeholder="t('common.search')"
                            :placeholder="t('views.dashboard.filterSensors')"
                        />
                    </span>
                    <span class="ml-3" v-tooltip.top="t('views.dashboard.filterByDataType')">
                        <UiMultiSelect
                            v-model="dashboard.dataTypes"
                            :groups="dataTypeGroups"
                            class="w-36"
                            :placeholder="t('views.dashboard.filterTypes')"
                        />
                    </span>
                </div>
                <div
                    v-if="dashboard.chartType == ChartType.TIME_CHART"
                    class="p-2 pr-0 flex flex-row bg-bg-one"
                >
                    <UiNumberInput
                        v-model="chartMinutes"
                        :min="chartMinutesMin"
                        :max="chartMinutesMax"
                        :step="1"
                        :suffix="t('common.minuteAbbr')"
                        v-tooltip.top="t('views.dashboard.timeRange')"
                    />
                    <axis-options class="h-10 ml-3" :dashboard="dashboard" />
                </div>
                <div
                    v-if="dashboard.chartType == ChartType.TABLE"
                    class="p-2 pr-0 flex leading-none items-center bg-bg-one"
                >
                    <UiButton
                        variant="outline"
                        @click="sensorTableRef?.resetStats()"
                        v-tooltip.top="t('components.sensorTable.resetStatsTooltip')"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiRestart"
                            :size="deviceStore.getREMSize(1.1)"
                        />
                        <span class="ml-1">{{ t('components.sensorTable.resetStats') }}</span>
                    </UiButton>
                </div>
                <div class="p-2 bg-bg-one">
                    <span v-tooltip.top="t('views.dashboard.chartType')">
                        <UiSelect
                            v-model="dashboard.chartType"
                            :options="chartTypeOptions"
                            class="w-32"
                        />
                    </span>
                </div>
            </template>
            <template #actions>
                <UiButton
                    v-if="isCustomSensor"
                    variant="outline"
                    v-tooltip.top="t('views.dashboard.openCustomSensor')"
                    @click="
                        router.push({
                            name: 'device-custom-sensor',
                            params: { customSensorID: props.channelName! },
                        })
                    "
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiThermometer"
                        :size="deviceStore.getREMSize(1.1)"
                    />
                    <span class="ml-1">{{ t('views.dashboard.editCustomSensor') }}</span>
                </UiButton>
                <template v-if="!sensorMode">
                    <UiButton
                        variant="ghost"
                        size="icon"
                        :class="{ '!text-accent': isHome }"
                        v-tooltip.top="t('views.dashboard.setAsHome')"
                        @click="setHome"
                    >
                        <svg-icon
                            type="mdi"
                            :path="isHome ? mdiHome : mdiHomeOutline"
                            :size="deviceStore.getREMSize(1.25)"
                        />
                    </UiButton>
                    <UiButton
                        variant="ghost"
                        size="icon"
                        v-tooltip.top="t('views.dashboard.duplicateDashboard')"
                        @click="duplicateDashboard"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiContentCopy"
                            :size="deviceStore.getREMSize(1.25)"
                        />
                    </UiButton>
                    <UiButton
                        v-if="settingsStore.dashboards.length > 1"
                        variant="ghost"
                        size="icon"
                        v-tooltip.top="t('views.dashboard.deleteDashboard')"
                        @click="deleteDashboard"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiTrashCanOutline"
                            :size="deviceStore.getREMSize(1.25)"
                        />
                    </UiButton>
                    <UiButton
                        variant="ghost"
                        size="icon"
                        v-tooltip.top="t('views.dashboard.fullPage')"
                        @click="toggleFullPage"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiOverscan"
                            :size="deviceStore.getREMSize(1.25)"
                        />
                    </UiButton>
                </template>
            </template>
            <!-- Inside #control-panel so the chart-height observer accounts for it. -->
            <health-warning
                v-if="sensorMode"
                kind="channel"
                :device-uid="props.deviceUID!"
                :channel-name="props.channelName!"
                class="w-full mx-2 mb-2"
            />
        </entity-page-header>
        <!-- z-index while full page: the wrapper is fixed but z-auto, so any
             positioned shell element with a z-index (the panel's scroll fades)
             paints over the chart. -->
        <Fullscreen
            v-model="fullPage"
            :teleport="true"
            :page-only="true"
            class="min-h-0 flex-1"
            :class="{ 'z-[1200]': fullPage }"
        >
            <div class="h-full" :class="{ 'full-page-wrapper': fullPage }">
                <!-- pr-2 plus the w-10 box put the icon where the header's
                     full-page button sits, so it does not jump on toggle. -->
                <div
                    v-if="fullPage"
                    class="fixed left-0 top-0 z-50 flex w-full flex-row justify-end pr-2 pt-0.5"
                >
                    <div
                        class="flex w-10 justify-center"
                        v-tooltip.top="t('views.dashboard.exitFullPage')"
                        @click="toggleFullPage"
                    >
                        <svg-icon
                            type="mdi"
                            class="text-text-color-secondary"
                            :path="mdiOverscan"
                            :size="deviceStore.getREMSize(1.325)"
                        />
                    </div>
                </div>
                <TimeChart
                    v-if="dashboard.chartType == ChartType.TIME_CHART"
                    :dashboard="viewDashboard"
                    :key="chartKey"
                    @line-set-changed="chartKey = uuidV4()"
                />
                <SensorTable
                    v-else-if="dashboard.chartType == ChartType.TABLE"
                    ref="sensorTableRef"
                    :dashboard="viewDashboard"
                    :key="'table' + chartKey"
                />
            </div>
        </Fullscreen>
    </div>
</template>

<style lang="scss" scoped>
.full-page-wrapper {
    width: 100%;
    height: 100%;
    background-color: rgb(var(--colors-bg-one));
}
</style>
