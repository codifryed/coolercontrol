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
import { onMounted, type Ref, ref, watch } from 'vue'
import InputNumber from 'primevue/inputnumber'
import Select from 'primevue/select'
import type { UID } from '@/models/Device.ts'
import { ChartType, Dashboard, DashboardDeviceChannel } from '@/models/Dashboard.ts'
import { $enum } from 'ts-enum-util'
import AxisOptions from '@/components/AxisOptions.vue'
import SensorTable from '@/components/SensorTable.vue'
import TimeChart from '@/components/TimeChart.vue'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'

interface Props {
    deviceUID: UID
    channelName: string
}

const props = defineProps<Props>()

const settingsStore = useSettingsStore()

const channelLabel =
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)
        ?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName
const createNewDashboard = (): Dashboard => {
    const dash = new Dashboard(channelLabel)
    dash.timeRangeSeconds = 300
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

const chartTypes = [...$enum(ChartType).values()]
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
const chartKey: Ref<string> = ref(uuidV4())
onMounted(async () => {
    addScrollEventListener()
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
    })
    watch(
        settingsStore.allUIDeviceSettings,
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
    )
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl">{{ channelLabel }}</div>
        <div class="flex justify-end">
            <div
                v-if="singleDashboard.chartType == ChartType.TIME_CHART"
                class="border-l-2 pr-4 py-2 pl-4 border-border-one flex flex-row"
            >
                <axis-options class="h-[2.375rem] mr-3" :dashboard="singleDashboard" />
                <InputNumber
                    placeholder="Minutes"
                    input-id="chart-minutes"
                    v-model="chartMinutes"
                    class="h-[2.375rem] chart-minutes"
                    suffix=" min"
                    show-buttons
                    :use-grouping="false"
                    :step="1"
                    :min="chartMinutesMin"
                    :max="chartMinutesMax"
                    button-layout="horizontal"
                    :allow-empty="false"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.bottom="'Time Range'"
                >
                    <template #incrementbuttonicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementbuttonicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
            </div>
            <div class="border-l-2 pr-4 py-2 pl-4 border-border-one">
                <Select
                    v-model="singleDashboard.chartType"
                    :options="chartTypes"
                    placeholder="Select a Chart Type"
                    class="w-32"
                    checkmark
                    dropdown-icon="pi pi-chart-bar"
                    scroll-height="400px"
                    v-tooltip.bottom="'Chart Type'"
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
