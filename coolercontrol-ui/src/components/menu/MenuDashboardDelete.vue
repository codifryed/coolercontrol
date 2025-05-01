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
import { mdiDeleteOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'

interface Props {
    dashboardUID: UID
}
const emit = defineEmits<{
    (e: 'deleted', dashboardUID: UID): void
}>()

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()
const { t } = useI18n()

const deleteDashboard = (): void => {
    const dashboardIndex: number = settingsStore.dashboards.findIndex(
        (dashboard) => dashboard.uid === props.dashboardUID,
    )
    if (dashboardIndex === -1) {
        console.error('Dashboard not found for removal: ', props.dashboardUID)
        return
    }
    confirm.require({
        message: t('views.dashboard.deleteDashboardConfirm', {
            name: settingsStore.dashboards[dashboardIndex].name,
        }),
        header: t('views.dashboard.deleteDashboard'),
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        accept: async () => {
            settingsStore.dashboards.splice(dashboardIndex, 1)
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('views.dashboard.dashboardDeleted'),
                life: 3000,
            })
            emit('deleted', props.dashboardUID)
        },
    })
}
</script>

<template>
    <div
        v-tooltip.top="{
            value: t('layout.menu.tooltips.delete'),
            disabled: settingsStore.dashboards.length < 2,
        }"
    >
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="deleteDashboard"
            :disabled="settingsStore.dashboards.length < 2"
        >
            <svg-icon type="mdi" :path="mdiDeleteOutline" :size="deviceStore.getREMSize(1.5)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
