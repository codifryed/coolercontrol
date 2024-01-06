<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import {
    CustomSensorType,
    type CustomSensor,
    CustomSensorMixFunctionType,
    CustomTempSourceData,
    CustomSensorTempSource,
} from '@/models/CustomSensor'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import Dropdown from 'primevue/dropdown'
import MultiSelect from 'primevue/multiselect'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import InputNumber from 'primevue/inputnumber'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiChip } from '@mdi/js'
import { inject, ref, type Ref } from 'vue'
import { $enum } from 'ts-enum-util'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { DeviceType } from '@/models/Device'

interface Props {
    customSensor: CustomSensor
    operation: 'add' | 'edit'
}

interface AvailableTemp {
    deviceUID: string // needed here as well for the dropdown selector
    tempName: string
    tempFrontendName: string
    tempExternalName: string
    lineColor: string
    weight: number
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

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const props: Props = dialogRef.value.data
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

// @ts-ignore
const sensorID: Ref<string> = ref(props.customSensor.id)
const selectedSensorType: Ref<CustomSensorType> = ref(props.customSensor.cs_type)
const sensorTypes = [...$enum(CustomSensorType).keys()]
const selectedMixFunction: Ref<CustomSensorMixFunctionType> = ref(props.customSensor.mix_function)
const mixFunctions = [...$enum(CustomSensorMixFunctionType).keys()]
const chosenTempSources: Ref<Array<AvailableTemp>> = ref([])

const tempSources: Ref<Array<AvailableTempSources>> = ref([])
const fillTempSources = () => {
    tempSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (
            device.status.temps.length === 0 ||
            device.info == null ||
            device.type === DeviceType.CUSTOM_SENSORS
        ) {
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
            if (deviceSettings.sensorsAndChannels.get(temp.name)!.hide) {
                continue
            }
            deviceSource.temps.push({
                deviceUID: device.uid,
                tempName: temp.name,
                tempFrontendName: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                tempExternalName: temp.external_name,
                lineColor: deviceSettings.sensorsAndChannels.get(temp.name)!.color,
                weight: 1,
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
    for (const customTempSourceData of props.customSensor.sources) {
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

const saveSensor = async () => {
    props.customSensor.cs_type = selectedSensorType.value
    props.customSensor.mix_function = selectedMixFunction.value
    const tempSources: Array<CustomTempSourceData> = []
    chosenTempSources.value.forEach((tempSource) =>
        tempSources.push(
            new CustomTempSourceData(
                new CustomSensorTempSource(tempSource.deviceUID, tempSource.tempName),
                tempSource.weight,
            ),
        ),
    )
    props.customSensor.sources = tempSources
    // includes UI refresh after successful save:
    if (props.operation === 'add') {
        await settingsStore.saveCustomSensor(props.customSensor)
    } else {
        // edit
        await settingsStore.updateCustomSensor(props.customSensor)
    }
}
</script>

<template>
    <div class="grid">
        <div class="col-fixed" style="width: 22rem">
            <span class="p-float-label mt-4">
                <InputText id="name" v-model="sensorID" class="w-full" disabled />
                <label for="name">ID</label>
            </span>
            <div class="p-float-label mt-5">
                <Dropdown
                    v-model="selectedSensorType"
                    inputId="dd-sensor-type"
                    :options="sensorTypes"
                    placeholder="Type"
                    class="w-full"
                    scroll-height="400px"
                />
                <label for="dd-sensor-type">Sensor Type</label>
            </div>
            <div class="p-float-label mt-5">
                <Dropdown
                    v-model="selectedMixFunction"
                    inputId="dd-mix-function"
                    :options="mixFunctions"
                    placeholder="Type"
                    class="w-full"
                    scroll-height="400px"
                />
                <label for="dd-mix-function">Mix Function</label>
            </div>
            <div class="p-float-label mt-4">
                <MultiSelect
                    v-model="chosenTempSources"
                    inputId="dd-temp-sources"
                    :options="tempSources"
                    option-label="tempFrontendName"
                    option-group-label="deviceName"
                    option-group-children="temps"
                    placeholder="Temp Sources"
                    :class="['w-full']"
                    scroll-height="400px"
                >
                    <template #optiongroup="slotProps">
                        <div class="flex align-items-center">
                            <svg-icon
                                type="mdi"
                                :path="mdiChip"
                                :size="deviceStore.getREMSize(1.3)"
                                class="mr-2"
                            />
                            <div>{{ slotProps.option.deviceName }}</div>
                        </div>
                    </template>
                    <template #option="slotProps">
                        <div class="flex align-items-center">
                            <span
                                class="pi pi-minus mr-2 ml-1"
                                :style="{ color: slotProps.option.lineColor }"
                            />{{ slotProps.option.tempFrontendName }}
                        </div>
                    </template>
                </MultiSelect>
                <label for="dd-temp-sources">Temp Sources</label>
            </div>
            <div
                v-if="selectedMixFunction === CustomSensorMixFunctionType.WeightedAvg"
                class="mt-4"
            >
                <DataTable :value="chosenTempSources">
                    <Column field="tempFrontendName" header="Temp Name">
                        <template #body="slotProps">
                            <span
                                class="pi pi-minus mr-2"
                                :style="{ color: slotProps.data.lineColor }"
                            />{{ slotProps.data.tempFrontendName }}
                        </template>
                    </Column>
                    <Column header="Weight">
                        <template #body="slotProps">
                            <InputNumber
                                v-model="slotProps.data.weight"
                                show-buttons
                                :min="1"
                                :max="254"
                                :input-style="{ width: '3.5rem' }"
                            />
                        </template>
                    </Column>
                </DataTable>
            </div>
            <div class="align-content-end">
                <div class="mt-6">
                    <Button label="Apply" class="w-full" @click="saveSensor">
                        <span class="p-button-label">Apply and Refresh</span>
                    </Button>
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
