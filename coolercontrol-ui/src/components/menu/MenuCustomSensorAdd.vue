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
import { mdiPlusCircleMultipleOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'
import { useDialog } from 'primevue/usedialog'
import { defineAsyncComponent, inject } from 'vue'
import { Emitter, EventType } from 'mitt'

interface Props {}

defineProps<Props>()
const deviceStore = useDeviceStore()
const { t } = useI18n()
const dialog = useDialog()
const customSensorWizard = defineAsyncComponent(() => import('../wizards/custom-sensor/Wizard.vue'))
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const addCustomSensor = (): void => {
    dialog.open(customSensorWizard, {
        props: {
            header: t('components.wizards.customSensor.new'),
            position: 'center',
            modal: true,
            dismissableMask: false,
        },
        data: {},
    })
}
emitter.on('custom-sensor-add', addCustomSensor)
</script>

<template>
    <div v-tooltip.top="{ value: t('layout.menu.tooltips.addCustomSensor') }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="addCustomSensor"
        >
            <svg-icon
                type="mdi"
                :path="mdiPlusCircleMultipleOutline"
                :size="deviceStore.getREMSize(1.5)"
            />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
