<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2024  Guy Boldon and contributors
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
import { useSettingsStore } from '@/stores/SettingsStore'
import { onMounted, onUnmounted, type Ref, ref, watch } from 'vue'
import InputNumber from 'primevue/inputnumber'
import Select from 'primevue/select'
import MultiSelect from 'primevue/multiselect'
import type { Color, UID } from '@/models/Device.ts'
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
import { mdiInformationSlabCircleOutline, mdiMemory, mdiOverscan } from '@mdi/js'
import SensorTable from '@/components/SensorTable.vue'
import TimeChart from '@/components/TimeChart.vue'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'
import ControlsOverview from '@/components/ControlsOverview.vue'
import { component as Fullscreen } from 'vue-fullscreen'
import { useI18n } from 'vue-i18n'

interface Props {
    dashboardUID?: UID
}

const props = defineProps<Props>()
const { t } = useI18n()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const dashboard: Dashboard =
    props.dashboardUID != null
        ? (settingsStore.dashboards.find((d) => d.uid === props.dashboardUID) ??
          settingsStore.dashboards[0])
        : settingsStore.dashboards[0] // show first dashboard by default

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
const controlSensorSources: Ref<Array<AvailableSensorSource>> = ref([])
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
const fillControlSensorSources = (): void => {
    controlSensorSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.info == null) continue
        if (device.info.channels.size === 0 && device.info.temps.size === 0) {
            continue
        }
        const sensors: Array<AvailableSensor> = []
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        device.info.channels.forEach((value: ChannelInfo, key: string) => {
            if (
                value.speed_options == null &&
                value.lcd_modes.length === 0 &&
                value.lighting_modes.length === 0
            )
                return
            const sensorSettings = deviceSettings.sensorsAndChannels.get(key)!
            sensors.push({
                name: key,
                deviceUID: device.uid,
                label: sensorSettings.name,
                color: sensorSettings.color,
            })
        })
        if (sensors.length === 0) continue
        controlSensorSources.value.push({
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            sensors: sensors,
        })
    }
}
fillControlSensorSources()
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

const updateDashboardSensorsFilter = (sensorSources: Array<AvailableSensor>): void => {
    const newSensorsFilter: Array<DashboardDeviceChannel> = []
    sensorSources.forEach((sensor: AvailableSensor) => {
        newSensorsFilter.push(new DashboardDeviceChannel(sensor.deviceUID, sensor.name))
    })
    dashboard.deviceChannelNames = newSensorsFilter
}

const addScrollEventListener = (): void => {
    // @ts-ignore
    document?.querySelector('.chart-minutes')?.addEventListener('wheel', chartMinutesScrolled)
}
const updateResponsiveGraphHeight = (): void => {
    const graphEl = document.getElementById('u-plot-chart')
    const controlPanel = document.getElementById('control-panel')
    if (graphEl != null && controlPanel != null) {
        if (fullPage.value) {
            graphEl.style.height = 'calc(100vh - 1rem)'
            return
        }
        const panelHeight = controlPanel.getBoundingClientRect().height
        if (panelHeight > 77) {
            // 5.5rem
            graphEl.style.height = `calc(100vh - (${panelHeight}px + 2rem))`
        } else {
            graphEl.style.height = 'calc(100vh - 5.75rem)'
        }
    }
}

const toggleFullPage = (): void => {
    fullPage.value = !fullPage.value
    updateResponsiveGraphHeight()
}

const chartKey: Ref<string> = ref(uuidV4())
onMounted(async () => {
    window.addEventListener('resize', updateResponsiveGraphHeight)
    setTimeout(updateResponsiveGraphHeight)

    addScrollEventListener()
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
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
})
</script>

<template>
    <div id="control-panel" class="flex border-b-4 border-border-one items-center justify-between">
        <div class="flex pl-4 py-2 text-2xl overflow-hidden">
            <span class="font-bold overflow-hidden overflow-ellipsis">{{ dashboard.name }}</span>
        </div>
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div
                v-if="dashboard.chartType == ChartType.TIME_CHART"
                class="p-2 flex leading-none items-center"
                v-tooltip.bottom="t('views.dashboard.mouseActions')"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiInformationSlabCircleOutline"
                    :size="deviceStore.getREMSize(1.25)"
                />
            </div>
            <div
                class="p-2 flex leading-none items-center"
                v-tooltip.bottom="t('views.dashboard.fullPage')"
                @click="toggleFullPage"
            >
                <svg-icon type="mdi" :path="mdiOverscan" :size="deviceStore.getREMSize(1.25)" />
            </div>
            <div class="p-2 pr-0 flex flex-row">
                <MultiSelect
                    v-model="chosenSensorSources"
                    :options="
                        dashboard.chartType !== ChartType.CONTROLS
                            ? sensorSources
                            : controlSensorSources
                    "
                    class="w-36 h-[2.375rem]"
                    :placeholder="t('views.dashboard.filterSensors')"
                    :filter-placeholder="t('common.search')"
                    filter
                    :dropdown-icon="
                        chosenSensorSources.length > 0 ? 'pi pi-filter' : 'pi pi-filter-slash'
                    "
                    option-label="label"
                    option-group-label="deviceName"
                    option-group-children="sensors"
                    scroll-height="40rem"
                    v-tooltip.bottom="t('views.dashboard.filterBySensor')"
                    @update:model-value="updateDashboardSensorsFilter"
                >
                    <template #optiongroup="slotProps">
                        <div class="flex items-center">
                            <svg-icon
                                type="mdi"
                                :path="mdiMemory"
                                :size="deviceStore.getREMSize(1.3)"
                                class="mr-2"
                            />
                            <div>{{ slotProps.option.deviceName }}</div>
                        </div>
                    </template>
                    <template #option="slotProps">
                        <div class="flex items-center">
                            <span
                                class="pi pi-minus mr-2 ml-1"
                                :style="{ color: slotProps.option.color }"
                            />
                            {{ slotProps.option.label }}
                        </div>
                    </template>
                </MultiSelect>
                <MultiSelect
                    v-if="dashboard.chartType != ChartType.CONTROLS"
                    v-model="dashboard.dataTypes"
                    :options="dataTypes"
                    class="ml-3 w-36 h-[2.375rem]"
                    :placeholder="t('views.dashboard.filterTypes')"
                    :dropdown-icon="
                        dashboard.dataTypes.length > 0 ? 'pi pi-filter' : 'pi pi-filter-slash'
                    "
                    scroll-height="16rem"
                    option-label="text"
                    option-value="value"
                    v-tooltip.bottom="t('views.dashboard.filterByDataType')"
                />
            </div>
            <div
                v-if="dashboard.chartType == ChartType.TIME_CHART"
                class="p-2 pr-0 flex flex-row bg-bg-one"
            >
                <InputNumber
                    :placeholder="t('views.dashboard.minutes')"
                    input-id="chart-minutes"
                    v-model="chartMinutes"
                    class="h-[2.375rem] chart-minutes"
                    :suffix="` ${t('views.dashboard.minutes').toLowerCase()}`"
                    show-buttons
                    :use-grouping="false"
                    :step="1"
                    :min="chartMinutesMin"
                    :max="chartMinutesMax"
                    button-layout="horizontal"
                    :allow-empty="false"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.bottom="t('views.dashboard.timeRange')"
                >
                    <template #incrementbuttonicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementbuttonicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
                <axis-options class="h-[2.375rem] ml-3" :dashboard="dashboard" />
            </div>
            <div class="p-2 bg-bg-one">
                <Select
                    v-model="dashboard.chartType"
                    :options="chartTypes"
                    :placeholder="t('views.dashboard.selectChartType')"
                    class="w-32 h-[2.375rem]"
                    checkmark
                    dropdown-icon="pi pi-chart-bar"
                    scroll-height="400px"
                    option-label="text"
                    option-value="value"
                    v-tooltip.bottom="t('views.dashboard.chartType')"
                />
            </div>
        </div>
    </div>
    <Fullscreen v-model="fullPage" :teleport="true" :page-only="true">
        <div :class="{ 'full-page-wrapper': fullPage }">
            <div
                v-if="fullPage"
                class="flex flex-row pt-0.5 fixed left-0 top-0 z-50 w-full justify-between"
            >
                <div />
                <div v-tooltip.left="t('views.dashboard.exitFullPage')" @click="toggleFullPage">
                    <svg-icon
                        type="mdi"
                        class="text-text-color-secondary"
                        :path="mdiOverscan"
                        :size="deviceStore.getREMSize(1.325)"
                    />
                </div>
                <div />
            </div>
            <TimeChart
                v-if="dashboard.chartType == ChartType.TIME_CHART"
                :dashboard="dashboard"
                :key="chartKey"
            />
            <SensorTable
                v-else-if="dashboard.chartType == ChartType.TABLE"
                :dashboard="dashboard"
                :key="'table' + chartKey"
            />
            <ControlsOverview
                v-else-if="dashboard.chartType == ChartType.CONTROLS"
                :dashboard="dashboard"
                :key="'controls' + chartKey"
            />
        </div>
    </Fullscreen>
</template>

<style lang="scss" scoped>
.full-page-wrapper {
    width: 100%;
    height: 100%;
    background-color: rgb(var(--colors-bg-one));
}
</style>
