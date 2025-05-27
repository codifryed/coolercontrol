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
import SvgIcon from '@jamescoyle/vue-icon'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { mdiArrowLeft, mdiContentSaveOutline } from '@mdi/js'
import Button from 'primevue/button'
import { CustomSensor } from '@/models/CustomSensor.ts'
import { SensorAndChannelSettings } from '@/models/UISettings.ts'
import { DeviceType, UID } from '@/models/Device.ts'

interface Props {
    customSensor: CustomSensor
    name: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

let customSensorsDeviceUID: UID = ''
for (const device of deviceStore.allDevices()) {
    if (device.type === DeviceType.CUSTOM_SENSORS) {
        customSensorsDeviceUID = device.uid
        break
    }
}
if (!customSensorsDeviceUID) {
    console.error("Custom Sensor Device UID NOT FOUND! This shouldn't happen.")
    throw new Error('Illegal State: Could not find Custom Sensor Device')
}
const deviceSettings = settingsStore.allUIDeviceSettings.get(customSensorsDeviceUID)!

const saveCustomSensor = async (): Promise<void> => {
    const successful = await settingsStore.saveCustomSensor(props.customSensor)
    if (successful) {
        // need to set the sensor name in the UI settings before we restart
        deviceSettings.sensorsAndChannels.set(props.customSensor.id, new SensorAndChannelSettings())
        deviceSettings.sensorsAndChannels.get(props.customSensor.id)!.userName = props.name
        emit('close')
        await deviceStore.waitAndReload(1)
    }
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <span class="text-xl text-center underline">{{
                t('components.wizards.fanControl.summary')
            }}</span>
            <div class="w-full text-lg">
                <p>
                    {{ t('components.wizards.customSensor.new') }}:
                    <span class="font-bold">{{ props.name }}</span>
                    <br />
                    {{ t('components.wizards.profile.willCreated') }}
                </p>
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 1)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="bg-accent/80 hover:!bg-accent w-32"
                :label="t('common.apply')"
                v-tooltip.bottom="t('views.speed.applySetting')"
                @click="saveCustomSensor"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiContentSaveOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
