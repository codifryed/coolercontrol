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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiInformationSlabCircleOutline } from '@mdi/js'
import { useSettingsStore } from '@/stores/SettingsStore'
import { onMounted, onUnmounted, type Ref, ref, watch } from 'vue'
import InputNumber from 'primevue/inputnumber'
import Select from 'primevue/select'
import type { UID } from '@/models/Device.ts'
import {
    ChartType,
    Dashboard,
    DashboardDeviceChannel,
    getLocalizedChartType,
} from '@/models/Dashboard.ts'
import { $enum } from 'ts-enum-util'
import AxisOptions from '@/components/AxisOptions.vue'
import SensorTable from '@/components/SensorTable.vue'
import TimeChart from '@/components/TimeChart.vue'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'

interface Props {
    deviceUID: UID
    channelName: string
}

const props = defineProps<Props>()
const { t } = useI18n()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const deviceLabel = settingsStore.allUIDeviceSettings.get(props.deviceUID)!.name
const channelLabel =
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)
        ?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName
const createNewDashboard = (): Dashboard => {
    const dash = new Dashboard(channelLabel)
    dash.timeRangeSeconds = 300
    // needed due to reduced default data type range:
    dash.dataTypes = []
    dash.deviceChannelNames.push(new DashboardDeviceChannel(props.deviceUID, props.channelName))
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.get(props.channelName)!.channelDashboard = dash
    return dash
}
const singleDashboard = ref(
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.get(props.channelName)!.channelDashboard ?? createNewDashboard(),
)

const chartTypes = [...$enum(ChartType).values()].map((type) => ({
    value: type,
    text: getLocalizedChartType(type),
}))
const chartMinutesMin: number = 1
const chartMinutesMax: number = 60
const chartMinutes: Ref<number> = ref(singleDashboard.value.timeRangeSeconds / 60)
const chartMinutesScrolled = (event: WheelEvent): void => {
    if (event.deltaY < 0) {
        if (chartMinutes.value < chartMinutesMax) chartMinutes.value += 1
    } else {
        if (chartMinutes.value > chartMinutesMin) chartMinutes.value -= 1
    }
}

const addScrollEventListener = (): void => {
    // @ts-ignore
    document?.querySelector('.chart-minutes')?.addEventListener('wheel', chartMinutesScrolled)
}
const chartMinutesChanged = (value: number): void => {
    singleDashboard.value.timeRangeSeconds = value * 60
}
const updateResponsiveGraphHeight = (): void => {
    const graphEl = document.getElementById('u-plot-chart')
    const controlPanel = document.getElementById('control-panel')
    if (graphEl != null && controlPanel != null) {
        const panelHeight = controlPanel.getBoundingClientRect().height
        if (panelHeight > 77) {
            // 5.5rem
            graphEl.style.height = `calc(100vh - (${panelHeight}px + 2rem))`
        } else {
            graphEl.style.height = 'calc(100vh - 5.75rem)'
        }
    }
}
const chartKey: Ref<string> = ref(uuidV4())
onMounted(async () => {
    window.addEventListener('resize', updateResponsiveGraphHeight)
    setTimeout(updateResponsiveGraphHeight)

    addScrollEventListener()
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
    })
    watch(
        settingsStore.allUIDeviceSettings,
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
    )
})
onUnmounted(() => {
    window.removeEventListener('resize', updateResponsiveGraphHeight)
})
</script>

<template>
    <div id="control-panel" class="flex border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl overflow-hidden flex">
            <span class="overflow-ellipsis overflow-hidden">{{ deviceLabel }}:&nbsp;</span>
            <span class="font-bold">{{ channelLabel }}</span>
        </div>
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div
                v-if="singleDashboard.chartType == ChartType.TIME_CHART"
                class="p-2 flex leading-none items-center"
                v-tooltip.bottom="t('views.singleDashboard.chartMouseActions')"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiInformationSlabCircleOutline"
                    :size="deviceStore.getREMSize(1.25)"
                />
            </div>
            <div
                v-if="singleDashboard.chartType == ChartType.TIME_CHART"
                class="p-2 pr-0 flex flex-row"
            >
                <InputNumber
                    placeholder="Minutes"
                    input-id="chart-minutes"
                    v-model="chartMinutes"
                    class="h-[2.375rem] chart-minutes"
                    :suffix="' ' + t('views.singleDashboard.minutes')"
                    show-buttons
                    :use-grouping="false"
                    :step="1"
                    :min="chartMinutesMin"
                    :max="chartMinutesMax"
                    button-layout="horizontal"
                    :allow-empty="false"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.bottom="t('views.singleDashboard.timeRange')"
                >
                    <template #incrementbuttonicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementbuttonicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
                <axis-options class="h-[2.375rem] ml-3" :dashboard="singleDashboard" />
            </div>
            <div class="p-2">
                <Select
                    v-model="singleDashboard.chartType"
                    :options="chartTypes"
                    :placeholder="t('views.dashboard.selectChartType')"
                    class="h-[2.375rem] w-32"
                    checkmark
                    dropdown-icon="pi pi-chart-bar"
                    scroll-height="400px"
                    option-label="text"
                    option-value="value"
                    v-tooltip.bottom="t('views.singleDashboard.chartType')"
                />
            </div>
        </div>
    </div>
    <TimeChart
        v-if="singleDashboard.chartType == ChartType.TIME_CHART"
        :dashboard="singleDashboard"
        :key="chartKey"
    />
    <SensorTable
        v-else-if="singleDashboard.chartType == ChartType.TABLE"
        :dashboard="singleDashboard"
        :key="'table' + chartKey"
    />
</template>

<style scoped></style>
