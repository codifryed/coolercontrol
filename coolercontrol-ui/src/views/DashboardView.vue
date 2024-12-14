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
import MultiSelect from 'primevue/multiselect'
import type { Color, UID } from '@/models/Device.ts'
import { ChartType, Dashboard, DashboardDeviceChannel, DataType } from '@/models/Dashboard.ts'
import { $enum } from 'ts-enum-util'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import AxisOptions from '@/components/AxisOptions.vue'
import { TempInfo } from '@/models/TempInfo.ts'
import { ChannelInfo } from '@/models/ChannelInfo.ts'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiMemory } from '@mdi/js'
import SensorTable from '@/components/SensorTable.vue'
import TimeChart from '@/components/TimeChart.vue'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'

interface Props {
    dashboardUID?: UID
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const dashboard: Dashboard =
    props.dashboardUID != null
        ? (settingsStore.dashboards.find((d) => d.uid === props.dashboardUID) ??
          settingsStore.dashboards[0])
        : settingsStore.dashboards[0] // show first dashboard by default

const chartTypes = [...$enum(ChartType).values()]
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
const dataTypes = [...$enum(DataType).values()]

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
            if (sensorSettings.hide) return
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
            if (sensorSettings.hide) return
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
const chartKey: Ref<string> = ref(uuidV4())
onMounted(async () => {
    addScrollEventListener()
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
    })
    // This forces a debounced chart redraw for any dashboard settings change:
    watch(
        [settingsStore.dashboards, settingsStore.allUIDeviceSettings],
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
    )
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl">{{ dashboard.name }}</div>
        <div class="flex justify-end">
            <div class="border-l-0 pr-4 py-2 border-border-one flex flex-row">
                <MultiSelect
                    v-model="chosenSensorSources"
                    :options="sensorSources"
                    class="w-36 h-[2.375rem]"
                    placeholder="Filter Sensors"
                    filter-placeholder="Search"
                    filter
                    :dropdown-icon="
                        chosenSensorSources.length > 0 ? 'pi pi-filter' : 'pi pi-filter-slash'
                    "
                    option-label="label"
                    option-group-label="deviceName"
                    option-group-children="sensors"
                    scroll-height="40rem"
                    v-tooltip.bottom="'Filter by Sensor'"
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
                    v-model="dashboard.dataTypes"
                    :options="dataTypes"
                    class="ml-3 w-36 h-[2.375rem]"
                    placeholder="Filter Types"
                    :dropdown-icon="
                        dashboard.dataTypes.length > 0 ? 'pi pi-filter' : 'pi pi-filter-slash'
                    "
                    v-tooltip.bottom="'Filter by Data Type'"
                />
            </div>
            <div
                v-if="dashboard.chartType == ChartType.TIME_CHART"
                class="border-l-2 pr-4 py-2 pl-4 border-border-one flex flex-row"
            >
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
                <axis-options class="h-[2.375rem] ml-3" :dashboard="dashboard" />
            </div>
            <div class="border-l-2 pr-4 py-2 pl-4 border-border-one">
                <Select
                    v-model="dashboard.chartType"
                    :options="chartTypes"
                    placeholder="Select a Chart Type"
                    class="w-32 h-[2.375rem]"
                    checkmark
                    dropdown-icon="pi pi-chart-bar"
                    scroll-height="400px"
                    v-tooltip.bottom="'Chart Type'"
                />
            </div>
        </div>
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
</template>

<style scoped></style>
