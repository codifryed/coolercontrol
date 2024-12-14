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
import { mdiContentDuplicate } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { Dashboard } from '@/models/Dashboard.ts'
import { UID } from '@/models/Device.ts'

interface Props {
    dashboardUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'added', dashboardUID: UID): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const duplicateDashboard = (): void => {
    const dashboardToDuplicate = settingsStore.dashboards.find(
        (dashboard) => dashboard.uid === props.dashboardUID,
    )
    if (dashboardToDuplicate == null) {
        console.error('Dashboard not found for duplication: ' + props.dashboardUID)
        return
    }
    const newDashboard = new Dashboard(`${dashboardToDuplicate.name} (copy)`)
    newDashboard.chartType = dashboardToDuplicate.chartType
    newDashboard.timeRangeSeconds = dashboardToDuplicate.timeRangeSeconds
    newDashboard.autoScaleDegree = dashboardToDuplicate.autoScaleDegree
    newDashboard.autoScaleFrequency = dashboardToDuplicate.autoScaleFrequency
    newDashboard.degreeMax = dashboardToDuplicate.degreeMax
    newDashboard.degreeMin = dashboardToDuplicate.degreeMin
    newDashboard.frequencyMax = dashboardToDuplicate.frequencyMax
    newDashboard.frequencyMin = dashboardToDuplicate.frequencyMin
    newDashboard.dataTypes = dashboardToDuplicate.dataTypes
    newDashboard.deviceChannelNames = dashboardToDuplicate.deviceChannelNames
    settingsStore.dashboards.push(newDashboard)
    emit('added', newDashboard.uid)
}
</script>

<template>
    <div v-tooltip.top="{ value: 'Duplicate' }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="duplicateDashboard"
        >
            <svg-icon type="mdi" :path="mdiContentDuplicate" :size="deviceStore.getREMSize(1.2)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
