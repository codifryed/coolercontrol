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
import Dropdown from 'primevue/dropdown'
import SelectButton, { SelectButtonChangeEvent } from 'primevue/selectbutton'
import InputNumber from 'primevue/inputnumber'
import TimeChart from '@/components/TimeChart.vue'
import SensorTable from '@/components/SensorTable.vue'

const settingsStore = useSettingsStore()

const chartTypes = ref(['TimeChart', 'Table'])
const chartMinutesMin: number = 1
const chartMinutesMax: number = 60
const chartMinutes: Ref<number> = ref(
    settingsStore.systemOverviewOptions.selectedTimeRange.seconds / 60,
)
const chartMinutesChanged = (value: number): void => {
    settingsStore.systemOverviewOptions.selectedTimeRange = {
        name: `${value} min`,
        seconds: value * 60,
    }
}
const chartMinutesScrolled = (event: WheelEvent): void => {
    if (event.deltaY < 0) {
        if (chartMinutes.value < chartMinutesMax) chartMinutes.value += 1
    } else {
        if (chartMinutes.value > chartMinutesMin) chartMinutes.value -= 1
    }
}

const lineThicknessOptions = ref([
    { optionSize: 1, value: 0.5 },
    { optionSize: 2, value: 1.0 },
    { optionSize: 3, value: 1.5 },
    { optionSize: 4, value: 2.0 },
    { optionSize: 6, value: 3.0 },
])

const tempEnabled = ref(settingsStore.systemOverviewOptions.temp)
const loadEnabled = ref(settingsStore.systemOverviewOptions.load)
const dutyEnabled = ref(settingsStore.systemOverviewOptions.duty)
const rpmEnabled = ref(settingsStore.systemOverviewOptions.rpm)
const freqEnabled = ref(settingsStore.systemOverviewOptions.freq)
const selectedChartOptions: Ref<Array<string>> = ref([])
if (settingsStore.systemOverviewOptions.temp) {
    selectedChartOptions.value.push('temp')
}
if (settingsStore.systemOverviewOptions.duty) {
    selectedChartOptions.value.push('duty')
}
if (settingsStore.systemOverviewOptions.load) {
    selectedChartOptions.value.push('load')
}
if (settingsStore.systemOverviewOptions.rpm) {
    selectedChartOptions.value.push('rpm')
}
if (settingsStore.systemOverviewOptions.freq) {
    selectedChartOptions.value.push('freq')
}
const chartOptions: Ref<Array<string>> = ref(['temp', 'duty', 'load', 'rpm', 'freq'])

const onChartOptionsChange = (event: SelectButtonChangeEvent) => {
    const newChoices = event.value as Array<string>
    for (const option of chartOptions.value) {
        if (newChoices.includes(option)) {
            switch (option) {
                case 'temp':
                    settingsStore.systemOverviewOptions.temp = true
                    tempEnabled.value = true
                    break
                case 'duty':
                    settingsStore.systemOverviewOptions.duty = true
                    dutyEnabled.value = true
                    break
                case 'load':
                    settingsStore.systemOverviewOptions.load = true
                    loadEnabled.value = true
                    break
                case 'rpm':
                    settingsStore.systemOverviewOptions.rpm = true
                    rpmEnabled.value = true
                    break
                case 'freq':
                    settingsStore.systemOverviewOptions.freq = true
                    freqEnabled.value = true
                    break
            }
        } else {
            switch (option) {
                case 'temp':
                    settingsStore.systemOverviewOptions.temp = false
                    tempEnabled.value = false
                    break
                case 'duty':
                    settingsStore.systemOverviewOptions.duty = false
                    dutyEnabled.value = false
                    break
                case 'load':
                    settingsStore.systemOverviewOptions.load = false
                    loadEnabled.value = false
                    break
                case 'rpm':
                    settingsStore.systemOverviewOptions.rpm = false
                    rpmEnabled.value = false
                    break
                case 'freq':
                    settingsStore.systemOverviewOptions.freq = false
                    freqEnabled.value = false
                    break
            }
        }
    }
}

const addScrollEventListener = (): void => {
    // @ts-ignore
    document?.querySelector('.chart-minutes')?.addEventListener('wheel', chartMinutesScrolled)
}

onMounted(async () => {
    addScrollEventListener()
    watch(settingsStore.systemOverviewOptions, () => {
        addScrollEventListener()
    })
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
    })
})
</script>

<template>
    <div class="card">
        <div class="flex justify-content-end flex-wrap card-container">
            <SelectButton
                v-if="settingsStore.systemOverviewOptions.selectedChartType === 'TimeChart'"
                v-model="selectedChartOptions"
                :options="chartOptions"
                multiple
                @change="onChartOptionsChange"
            >
                <template #option="slotProps">
                    {{ slotProps.option }}
                </template>
            </SelectButton>
            <Dropdown
                v-if="settingsStore.systemOverviewOptions.selectedChartType === 'TimeChart'"
                v-model="settingsStore.systemOverviewOptions.timeChartLineScale"
                :options="lineThicknessOptions"
                option-label="optionSize"
                option-value="value"
                placeholder="Select a Line Thickness"
                class="w-full md:w-8rem ml-2"
                scroll-height="400px"
            >
                <template #value="slotProps">
                    <div class="align-content-center h-full w-full">
                        <div
                            :style="`border-bottom: ${slotProps.value * 2}px solid var(--text-color)`"
                        />
                    </div>
                </template>
                <template #option="slotProps">
                    <div
                        :style="`border-bottom: ${slotProps.option.optionSize}px solid var(--text-color)`"
                    />
                </template>
            </Dropdown>
            <InputNumber
                placeholder="Minutes"
                input-id="chart-minutes"
                v-model="chartMinutes"
                mode="decimal"
                class="chart-minutes w-full md:w-8rem ml-2"
                suffix=" min"
                show-buttons
                :step="1"
                :min="chartMinutesMin"
                :max="chartMinutesMax"
                :input-style="{ width: '60px' }"
                :allow-empty="false"
            />
            <Dropdown
                v-model="settingsStore.systemOverviewOptions.selectedChartType"
                :options="chartTypes"
                placeholder="Select a Chart Type"
                class="w-full md:w-10rem ml-2"
                scroll-height="400px"
            />
        </div>
        <TimeChart
            v-if="settingsStore.systemOverviewOptions.selectedChartType === 'TimeChart'"
            :temp="tempEnabled"
            :load="loadEnabled"
            :duty="dutyEnabled"
            :rpm="rpmEnabled"
            :freq="freqEnabled"
            :key="
                'TimeChart' +
                String(tempEnabled) +
                String(loadEnabled) +
                String(dutyEnabled) +
                String(rpmEnabled) +
                String(freqEnabled)
            "
        />
        <SensorTable
            v-else-if="settingsStore.systemOverviewOptions.selectedChartType === 'Table'"
        />
    </div>
</template>

<style scoped></style>
