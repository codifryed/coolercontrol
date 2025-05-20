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
import { mdiArrowLeft, mdiMemory } from '@mdi/js'
import Button from 'primevue/button'
import { UID } from '@/models/Device.ts'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import Select from 'primevue/select'
import { ref, Ref, watch } from 'vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { ProfileTempSource } from '@/models/Profile.ts'
import { storeToRefs } from 'pinia'

interface Props {
    deviceUID: UID
    channelName: string
    name: string
    tempSource: ProfileTempSource | undefined
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'tempSource', tempSource: ProfileTempSource): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()

interface AvailableTemp {
    deviceUID: string // needed here as well for the dropdown selector
    tempName: string
    tempFrontendName: string
    lineColor: string
    temp: string
}

interface AvailableTempSources {
    deviceUID: string
    deviceName: string
    profileMinLength: number
    profileMaxLength: number
    tempMin: number
    tempMax: number
    temps: Array<AvailableTemp>
}

const chosenTemp: Ref<AvailableTemp | undefined> = ref()
const tempSources: Ref<Array<AvailableTempSources>> = ref([])
const fillTempSources = () => {
    tempSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.status.temps.length === 0 || device.info == null) {
            continue
        }
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        const deviceSource: AvailableTempSources = {
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            profileMinLength: device.info.profile_min_length,
            profileMaxLength: device.info.profile_max_length,
            tempMin: device.info.temp_min,
            tempMax: device.info.temp_max,
            temps: [],
        }
        for (const temp of device.status.temps) {
            deviceSource.temps.push({
                deviceUID: device.uid,
                tempName: temp.name,
                tempFrontendName: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                lineColor: deviceSettings.sensorsAndChannels.get(temp.name)!.color,
                temp: temp.temp.toFixed(1),
            })
        }
        if (deviceSource.temps.length === 0) {
            continue // when all of a devices temps are hidden
        }
        tempSources.value.push(deviceSource)
    }
}
fillTempSources()

// set chosenTemp on startup if set in profile
if (props.tempSource != null) {
    for (const availableTempSource of tempSources.value) {
        if (availableTempSource.deviceUID !== props.tempSource.device_uid) {
            continue
        }
        for (const availableTemp of availableTempSource.temps) {
            if (
                availableTemp.deviceUID === props.tempSource.device_uid &&
                availableTemp.tempName === props.tempSource.temp_name
            ) {
                chosenTemp.value = availableTemp
                break
            }
        }
    }
}
const nextStep = () => {
    if (chosenTemp.value == null) {
        return
    }
    const newTempSource = new ProfileTempSource(
        chosenTemp.value.tempName,
        chosenTemp.value.deviceUID,
    )
    emit('tempSource', newTempSource)
    emit('nextStep', 9)
}

const updateTemps = () => {
    for (const tempDevice of tempSources.value) {
        for (const availableTemp of tempDevice.temps) {
            availableTemp.temp =
                currentDeviceStatus.value.get(availableTemp.deviceUID)!.get(availableTemp.tempName)!
                    .temp || '0.0'
        }
    }
}

watch(settingsStore.allUIDeviceSettings, () => {
    // update all temp sources:
    fillTempSources()
})
watch(currentDeviceStatus, () => {
    updateTemps()
})
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div class="w-full mb-2">
                {{ t('components.wizards.fanControl.newGraphProfile') }}:
                <span class="font-bold">{{ props.name }}</span>
            </div>
            <div class="mt-0 flex flex-col">
                <small class="ml-2 mb-1 font-light text-sm">
                    {{ t('views.profiles.tempSource') }}
                </small>
                <Select
                    v-model="chosenTemp"
                    :options="tempSources"
                    class="w-full h-11 !justify-end"
                    option-label="tempFrontendName"
                    option-group-label="deviceName"
                    option-group-children="temps"
                    :placeholder="t('views.profiles.tempSource')"
                    :filter-placeholder="t('common.search')"
                    filter
                    checkmark
                    scroll-height="40rem"
                    :invalid="chosenTemp == null"
                    dropdown-icon="pi pi-inbox"
                >
                    <template #optiongroup="slotProps">
                        <div class="flex items-center">
                            <svg-icon
                                type="mdi"
                                :path="mdiMemory"
                                :size="deviceStore.getREMSize(1.3)"
                                class="mr-2"
                            />
                            <div>{{ slotProps.option.deviceName }}</div>
                        </div>
                    </template>
                    <template #option="slotProps">
                        <div class="flex w-full items-center justify-between">
                            <div>
                                <span
                                    class="pi pi-minus mr-2 ml-1"
                                    :style="{ color: slotProps.option.lineColor }"
                                />{{ slotProps.option.tempFrontendName }}
                            </div>
                            <div>
                                {{ slotProps.option.temp + ' °' }}
                            </div>
                        </div>
                    </template>
                </Select>
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 3)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="w-24 bg-bg-one"
                :label="t('common.next')"
                :disabled="chosenTemp == null"
                @click="nextStep"
            />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
