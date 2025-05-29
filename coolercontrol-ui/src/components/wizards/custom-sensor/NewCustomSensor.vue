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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiFolderSearchOutline, mdiMemory } from '@mdi/js'
import {
    CustomSensor,
    CustomSensorMixFunctionType,
    CustomSensorTempSource,
    CustomSensorType,
    CustomTempSourceData,
    getCustomSensorTypeDisplayName,
    getCustomSensorMixFunctionTypeDisplayName,
} from '@/models/CustomSensor.ts'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import InputNumber from 'primevue/inputnumber'
import { onMounted, ref, type Ref, watch, computed } from 'vue'
import { $enum } from 'ts-enum-util'
import { DEFAULT_NAME_STRING_LENGTH, useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { DeviceType, UID } from '@/models/Device.ts'
import { storeToRefs } from 'pinia'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'
import { useI18n } from 'vue-i18n'
import Select from 'primevue/select'
import MultiSelect from 'primevue/multiselect'

interface Props {
    customSensor?: CustomSensor
    name: string
}

interface AvailableTemp {
    deviceUID: string // needed here as well for the dropdown selector
    tempName: string
    tempFrontendName: string
    lineColor: string
    weight: number
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

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'newCustomSensor', sensor: CustomSensor): void
    (e: 'name', name: string): void
    (e: 'close'): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const { t } = useI18n()

const customSensorIdNumbers: Array<number> = []
let customSensorsDeviceUID: UID = ''
for (const device of deviceStore.allDevices()) {
    if (device.type === DeviceType.CUSTOM_SENSORS) {
        customSensorsDeviceUID = device.uid
        for (const temp of device.status.temps) {
            customSensorIdNumbers.push(Number(temp.name.replace(/^\D+/g, '')))
        }
        customSensorIdNumbers.sort()
        break
    }
}
if (!customSensorsDeviceUID) {
    console.error("Custom Sensor Device UID NOT FOUND! This shouldn't happen.")
    throw new Error('Illegal State: Could not find Custom Sensor Device')
}

const createNewCustomSensor = (): CustomSensor => {
    const newSensorNumber =
        customSensorIdNumbers.length === 0
            ? 1
            : customSensorIdNumbers[customSensorIdNumbers.length - 1] + 1
    return new CustomSensor(`sensor${newSensorNumber}`)
}
const customSensor: CustomSensor =
    props.customSensor === undefined ? createNewCustomSensor() : props.customSensor

const sensorName: Ref<string> = ref(customSensor.id)
const selectedSensorType: Ref<CustomSensorType> = ref(customSensor.cs_type)
const selectedMixFunction: Ref<CustomSensorMixFunctionType> = ref(customSensor.mix_function)

// Generate options with localized display names
const sensorTypeOptions = computed(() => {
    return [...$enum(CustomSensorType).values()].map((type) => ({
        value: type,
        label: getCustomSensorTypeDisplayName(type),
    }))
})

const mixFunctionTypeOptions = computed(() => {
    return [...$enum(CustomSensorMixFunctionType).values()].map((type) => ({
        value: type,
        label: getCustomSensorMixFunctionTypeDisplayName(type),
    }))
})

const chosenTempSources: Ref<Array<AvailableTemp>> = ref([])
const filePath: Ref<string | undefined> = ref(customSensor.file_path)
const tempSources: Ref<Array<AvailableTempSources>> = ref([])
const fillTempSources = (): void => {
    tempSources.value.length = 0
    // const customSensors: Array<CustomSensor> = await settingsStore.getCustomSensors()
    for (const device of deviceStore.allDevices()) {
        if (
            device.status.temps.length === 0 ||
            device.info == undefined ||
            device.type === DeviceType.CUSTOM_SENSORS
        ) {
            continue
        }
        // todo: if this is requested in the future, but requires quite a bit of work to make sure
        //   it works correctly in the backend
        // if (
        //     device.type === DeviceType.CUSTOM_SENSORS &&
        //     customSensors.find((cs) => cs.cs_type === CustomSensorType.File) === undefined
        // ) {
        //     continue // only include file based sensors if there are any
        // }
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
            // if (
            //     device.type === DeviceType.CUSTOM_SENSORS &&
            //     customSensors.find(
            //         (cs) => cs.id === temp.name && cs.cs_type === CustomSensorType.Mix,
            //     ) !== undefined
            // ) {
            //     continue
            // }
            deviceSource.temps.push({
                deviceUID: device.uid,
                tempName: temp.name,
                tempFrontendName: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                lineColor: deviceSettings.sensorsAndChannels.get(temp.name)!.color,
                weight: 1,
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
const fillChosenTempSources = () => {
    chosenTempSources.value.length = 0
    for (const customTempSourceData of customSensor.sources) {
        for (const availableTempSource of tempSources.value) {
            if (availableTempSource.deviceUID === customTempSourceData.temp_source.device_uid) {
                for (const availableTemp of availableTempSource.temps) {
                    if (availableTemp.tempName === customTempSourceData.temp_source.temp_name) {
                        availableTemp.weight = customTempSourceData.weight
                        chosenTempSources.value.push(availableTemp)
                    }
                }
            }
        }
    }
}
fillChosenTempSources()

const nextStep = async (): Promise<void> => {
    customSensor.cs_type = selectedSensorType.value
    customSensor.mix_function = selectedMixFunction.value
    const tempSources: Array<CustomTempSourceData> = []
    if (customSensor.cs_type === CustomSensorType.File) {
        customSensor.file_path = filePath.value
    } else if (customSensor.cs_type === CustomSensorType.Mix) {
        customSensor.file_path = undefined
        chosenTempSources.value.forEach((tempSource) =>
            tempSources.push(
                new CustomTempSourceData(
                    new CustomSensorTempSource(tempSource.deviceUID, tempSource.tempName),
                    tempSource.weight,
                ),
            ),
        )
    }
    customSensor.sources = tempSources
    emit('name', sensorName.value)
    emit('newCustomSensor', customSensor)
    emit('nextStep', 2)
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

const changeSensorType = (event: any): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    selectedSensorType.value = event.value
}
const changeMixFunction = (event: any): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    selectedMixFunction.value = event.value
}

const chartKey: Ref<string> = ref(uuidV4())
const fileBrowse = async (): Promise<void> => {
    // @ts-ignore
    const ipc = window.ipc
    filePath.value = await ipc.filePathDialog(t('views.customSensors.selectCustomSensorFile'))
}

const nameInvalid = computed(() => {
    return sensorName.value.length < 1 || sensorName.value.length > DEFAULT_NAME_STRING_LENGTH
})

onMounted(async () => {
    watch(currentDeviceStatus, () => {
        updateTemps()
    })
    watch(settingsStore.allUIDeviceSettings, async () => {
        fillTempSources()
        fillChosenTempSources()
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true })()
    })
})
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div class="mt-0">
                <small class="ml-3 font-light text-sm"> {{ t('common.name') }}: </small>
                <div class="mt-0 flex flex-col">
                    <InputText
                        v-model="sensorName"
                        :placeholder="t('common.name')"
                        ref="inputArea"
                        id="property-name"
                        class="w-full h-11"
                        :invalid="nameInvalid"
                        :input-style="{ background: 'rgb(var(--colors-bg-one))' }"
                    />
                </div>
            </div>
            <div class="mt-0">
                <small class="ml-3 font-light text-sm">
                    {{ t('views.customSensors.sensorType') }}
                </small>
                <Select
                    :model-value="selectedSensorType"
                    :options="sensorTypeOptions"
                    class="w-full mr-3 h-11 bg-bg-one !justify-end"
                    checkmark
                    :placeholder="t('views.customSensors.type')"
                    @change="changeSensorType"
                    option-label="label"
                    option-value="value"
                />
            </div>
            <div v-if="selectedSensorType === CustomSensorType.Mix" class="mt-0">
                <small class="ml-3 font-light text-sm">
                    {{ t('views.customSensors.mixFunction') }}
                </small>
                <Select
                    :model-value="selectedMixFunction"
                    :options="mixFunctionTypeOptions"
                    checkmark
                    :placeholder="t('views.customSensors.type')"
                    class="w-full mr-3 h-11 bg-bg-one !justify-end"
                    v-tooltip.bottom="t('views.customSensors.howCalculateValue')"
                    @change="changeMixFunction"
                    option-label="label"
                    option-value="value"
                />
            </div>
            <div
                v-else-if="selectedSensorType === CustomSensorType.File"
                class="flex flex-col mt-1"
            >
                <small class="ml-3 mb-1 font-light text-sm">
                    {{ t('views.customSensors.tempFile') }}
                </small>
                <InputText
                    v-model="filePath"
                    class="w-full h-11"
                    :placeholder="'/tmp/your_temp_file'"
                    :invalid="!filePath"
                />
                <div v-if="deviceStore.isQtApp()">
                    <Button
                        class="mt-2 w-full h-11"
                        :label="t('views.customSensors.browse')"
                        v-tooltip.bottom="t('views.customSensors.browseCustomSensorFile')"
                        @click="fileBrowse"
                    >
                        <svg-icon
                            class="outline-0 mt-[-0.25rem]"
                            type="mdi"
                            :path="mdiFolderSearchOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('views.customSensors.browse') }}
                    </Button>
                </div>
                <div
                    class="whitespace-pre mt-4"
                    v-html="t('views.customSensors.filePathTooltip')"
                />
            </div>
            <div
                v-if="selectedSensorType === CustomSensorType.Mix"
                class="flex flex-col mt-0 w-full"
            >
                <div class="w-full">
                    <small class="ml-3 font-light text-sm">
                        {{ t('views.customSensors.tempSources') }}
                    </small>
                    <MultiSelect
                        v-model="chosenTempSources"
                        :options="tempSources"
                        class="w-full h-11 bg-bg-one items-center"
                        filter
                        checkmark
                        option-label="tempFrontendName"
                        option-group-label="deviceName"
                        option-group-children="temps"
                        :filter-placeholder="t('common.search')"
                        :invalid="chosenTempSources.length === 0"
                        scroll-height="40rem"
                        dropdown-icon="pi pi-microchip"
                        :placeholder="t('views.customSensors.tempSources')"
                        v-tooltip.bottom="{
                            escape: false,
                            value: t('views.customSensors.tempSourcesTooltip'),
                        }"
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
                            <div class="flex items-center w-full justify-between">
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
                    </MultiSelect>
                </div>
                <div
                    v-if="selectedMixFunction === CustomSensorMixFunctionType.WeightedAvg"
                    class="mt-4"
                >
                    <small class="ml-3 font-light text-sm">
                        {{ t('views.customSensors.tempWeights') }}
                    </small>
                    <DataTable :value="chosenTempSources">
                        <Column
                            field="tempFrontendName"
                            :header="t('views.customSensors.tempName')"
                            body-class="w-full"
                        >
                            <template #body="slotProps">
                                <span
                                    class="pi pi-minus mr-2"
                                    :style="{ color: slotProps.data.lineColor }"
                                />{{ slotProps.data.tempFrontendName }}
                            </template>
                        </Column>
                        <Column :header="t('views.customSensors.weight')">
                            <template #body="slotProps">
                                <InputNumber
                                    v-model="slotProps.data.weight"
                                    show-buttons
                                    :min="1"
                                    :max="254"
                                    button-layout="horizontal"
                                    :input-style="{ width: '3rem' }"
                                >
                                    <template #incrementicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </template>
                        </Column>
                    </DataTable>
                </div>
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" :label="t('common.cancel')" @click="emit('close')" />
            <Button
                class="w-24 bg-bg-one"
                :label="t('common.next')"
                :disabled="
                    nameInvalid ||
                    (selectedSensorType === CustomSensorType.Mix &&
                        chosenTempSources.length === 0) ||
                    (selectedSensorType === CustomSensorType.File && !filePath)
                "
                @click="nextStep"
            />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
