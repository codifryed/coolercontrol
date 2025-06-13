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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiChartBoxPlusOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { defineAsyncComponent, inject } from 'vue'
import { Emitter, EventType } from 'mitt'
import { useI18n } from 'vue-i18n'
import { useDialog } from 'primevue/usedialog'

interface Props {}

defineProps<Props>()

const deviceStore = useDeviceStore()
const dialog = useDialog()
const { t } = useI18n()

const dashboardWizard = defineAsyncComponent(() => import('../wizards/dashboard/Wizard.vue'))
const addDashboard = async (): Promise<void> => {
    dialog.open(dashboardWizard, {
        props: {
            header: t('layout.menu.tooltips.addDashboard'),
            position: 'center',
            modal: true,
            dismissableMask: true,
        },
        data: {},
    })
    // Now handled by the wizard
    // const newDashboard = new Dashboard('New Dashboard')
    // settingsStore.dashboards.push(newDashboard)
    // emit('added', newDashboard.uid)
    // await router.push({ name: 'dashboards', params: { dashboardUID: newDashboard.uid } })
}
// be able to add a dashboard from the side menu add button:
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
emitter.on('dashboard-add', addDashboard)
</script>

<template>
    <div v-tooltip.top="{ value: t('layout.menu.tooltips.addDashboard') }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="addDashboard"
        >
            <svg-icon
                type="mdi"
                :path="mdiChartBoxPlusOutline"
                :size="deviceStore.getREMSize(1.5)"
            />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
