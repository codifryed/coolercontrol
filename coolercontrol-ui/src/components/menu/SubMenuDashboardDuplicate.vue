<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2025  Guy Boldon and contributors
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
import { useI18n } from 'vue-i18n'

interface Props {
    dashboardUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'added', dashboardUID: UID): void
    (e: 'close'): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()

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
    emit('close')
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click="duplicateDashboard"
    >
        <svg-icon
            class="outline-0 !cursor-pointer"
            type="mdi"
            :path="mdiContentDuplicate"
            :size="deviceStore.getREMSize(1.5)"
        />
        <span class="ml-1.5">
            {{ t('layout.menu.tooltips.duplicate') }}
        </span>
    </Button>
</template>

<style scoped lang="scss"></style>
