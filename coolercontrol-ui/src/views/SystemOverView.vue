<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import { useSettingsStore } from '@/stores/SettingsStore'
import { type Ref, ref } from 'vue'
import Dropdown from 'primevue/dropdown'
import SelectButton, { SelectButtonChangeEvent } from 'primevue/selectbutton'
import TimeChart from '@/components/TimeChart.vue'
import SensorTable from '@/components/SensorTable.vue'

const settingsStore = useSettingsStore()

const chartTypes = ref(['TimeChart', 'Table'])
const timeRanges: Ref<Array<{ name: string; seconds: number }>> = ref([
    { name: '1 min', seconds: 60 },
    { name: '5 min', seconds: 300 },
    { name: '15 min', seconds: 900 },
    { name: '30 min', seconds: 1800 },
])

const tempEnabled = ref(settingsStore.systemOverviewOptions.temp)
const loadEnabled = ref(settingsStore.systemOverviewOptions.load)
const dutyEnabled = ref(settingsStore.systemOverviewOptions.duty)
const rpmEnabled = ref(settingsStore.systemOverviewOptions.rpm)
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
const chartOptions: Ref<Array<string>> = ref(['temp', 'duty', 'load', 'rpm'])

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
            }
        }
    }
}
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
                v-model="settingsStore.systemOverviewOptions.selectedTimeRange"
                :options="timeRanges"
                placeholder="Select a Time Range"
                option-label="name"
                class="w-full md:w-10rem ml-2"
                scroll-height="400px"
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
            :key="
                'TimeChart' +
                String(tempEnabled) +
                String(loadEnabled) +
                String(dutyEnabled) +
                String(rpmEnabled)
            "
        />
        <SensorTable
            v-else-if="settingsStore.systemOverviewOptions.selectedChartType === 'Table'"
        />
    </div>
</template>

<style scoped></style>
