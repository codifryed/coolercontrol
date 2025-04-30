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
import { mdiHomeAnalytics } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { computed, Ref } from 'vue'
import { useI18n } from 'vue-i18n'

interface Props {
    dashboardUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'rearrange'): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()

const chosenDashboardIndex: Ref<number> = computed(() =>
    settingsStore.dashboards.findIndex((dashboard) => dashboard.uid === props.dashboardUID),
)

const setDashboardAsHome = (): void => {
    if (chosenDashboardIndex.value < 0) {
        console.error('Dashboard not found for home dashboard: ' + props.dashboardUID)
        return
    }
    const removedDashboards = settingsStore.dashboards.splice(chosenDashboardIndex.value, 1)
    settingsStore.dashboards.unshift(removedDashboards[0])
    emit('rearrange')
}
</script>

<template>
    <div v-tooltip.top="{ value: t('views.dashboard.setAsHome'), disabled: chosenDashboardIndex === 0 }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="setDashboardAsHome"
            :disabled="chosenDashboardIndex === 0"
        >
            <svg-icon type="mdi" :path="mdiHomeAnalytics" :size="deviceStore.getREMSize(1.2)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
