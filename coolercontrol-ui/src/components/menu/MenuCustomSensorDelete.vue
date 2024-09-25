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

interface Props {
    deviceUID: UID // the main CustomSensor device has a UUID
    customSensorID: string // individual sensors are like channel on normal devices
}
const emit = defineEmits<{
    (e: 'deleted', customSensorID: string): void
}>()

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
// const toast = useToast()

const deviceSettings = settingsStore.allUIDeviceSettings.get(props.deviceUID)!

const deleteCustomSensor = (): void => {
    const currentName: string =
        deviceSettings.sensorsAndChannels.get(props.customSensorID)?.name ?? props.customSensorID
    // original:
    confirm.require({
        message: `Are you sure you want to delete the custom sensor: "${currentName}"?`,
        header: 'Delete Custom Sensor',
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        accept: async () => {
            await settingsStore.deleteCustomSensor(props.deviceUID, props.customSensorID)
            emit('deleted', props.deviceUID)
            // todo: no toast needed I think
            // toast.add({
            //     severity: 'success',
            //     summary: 'Success',
            //     detail: 'Dashboard Deleted',
            //     life: 3000,
            // })
        },
    })
}
</script>

<template>
    <div v-tooltip.top="{ value: 'Delete Sensor' }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="deleteCustomSensor"
        >
            <svg-icon type="mdi" :path="mdiDeleteOutline" :size="deviceStore.getREMSize(1.5)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
