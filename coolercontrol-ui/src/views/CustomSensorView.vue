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
import { mdiContentSaveOutline, mdiMemory } from '@mdi/js'
import {
    CustomSensor,
    CustomSensorMixFunctionType,
    CustomSensorTempSource,
    CustomSensorType,
    CustomTempSourceData,
} from '@/models/CustomSensor.ts'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import InputNumber from 'primevue/inputnumber'
import { onMounted, ref, type Ref, watch } from 'vue'
import { $enum } from 'ts-enum-util'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { DeviceType, UID } from '@/models/Device.ts'
import { storeToRefs } from 'pinia'
import { ChannelViewType, SensorAndChannelSettings } from '@/models/UISettings.ts'
import Listbox, { ListboxChangeEvent } from 'primevue/listbox'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { onBeforeRouteLeave, onBeforeRouteUpdate } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'
import Select from 'primevue/select'
import { ChartType, Dashboard, DashboardDeviceChannel } from '@/models/Dashboard.ts'
import TimeChart from '@/components/TimeChart.vue'
import SensorTable from '@/components/SensorTable.vue'
import AxisOptions from '@/components/AxisOptions.vue'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'

interface Props {
    customSensorID?: string
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
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const confirm = useConfirm()

let contextIsDirty: boolean = false
const shouldCreateSensor: boolean = !props.customSensorID
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
const deviceSettings = settingsStore.allUIDeviceSettings.get(customSensorsDeviceUID)!

const collectCustomSensor = async (): Promise<CustomSensor> => {
    if (shouldCreateSensor) {
        const newSensorNumber =
            customSensorIdNumbers.length === 0
                ? 1
                : customSensorIdNumbers[customSensorIdNumbers.length - 1] + 1
        return new CustomSensor(`sensor${newSensorNumber}`)
    } else {
        const foundSensor = await settingsStore.getCustomSensor(props.customSensorID!)
        if (foundSensor == undefined) {
            throw new Error(
                `Illegal State: Could not find Custom Sensor with ID: ${props.customSensorID}`,
            )
        }
        return foundSensor
    }
}
const customSensor: CustomSensor = await collectCustomSensor()

// @ts-ignore
const sensorID: Ref<string> = ref(customSensor.id)
const currentName: string =
    deviceSettings.sensorsAndChannels.get(customSensor.id as string)?.name ?? sensorID.value
const isUserName: boolean =
    deviceSettings.sensorsAndChannels.get(customSensor.id as string)?.userName != undefined
const sensorName: Ref<string> = ref(isUserName ? currentName : '')
const selectedSensorType: Ref<CustomSensorType> = ref(customSensor.cs_type)
const sensorTypes = [...$enum(CustomSensorType).keys()]
const selectedMixFunction: Ref<CustomSensorMixFunctionType> = ref(customSensor.mix_function)
const mixFunctions = [...$enum(CustomSensorMixFunctionType).keys()]
const chosenTempSources: Ref<Array<AvailableTemp>> = ref([])
const filePath: Ref<string | undefined> = ref(customSensor.file_path)
const chosenViewType: Ref<ChannelViewType> = ref(
    deviceSettings.sensorsAndChannels.get(customSensor.id as string)?.viewType ??
        ChannelViewType.Control,
)
const viewTypeOptions = [...$enum(ChannelViewType).keys()]

const tempSources: Ref<Array<AvailableTempSources>> = ref([])
const fillTempSources = async (): Promise<void> => {
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
            if (deviceSettings.sensorsAndChannels.get(temp.name)!.hide) {
                continue
            }
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
await fillTempSources()
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

const saveSensor = async (): Promise<void> => {
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

    if (shouldCreateSensor) {
        const successful = await settingsStore.saveCustomSensor(customSensor)
        if (successful) {
            // need to set the sensor name in the UI settings before we restart
            deviceSettings.sensorsAndChannels.set(
                customSensor.id as string,
                new SensorAndChannelSettings(),
            )
            if (sensorName.value) {
                sensorName.value = deviceStore.sanitizeString(sensorName.value)
                deviceSettings.sensorsAndChannels.get(customSensor.id as string)!.userName =
                    sensorName.value
            }
            await deviceStore.waitAndReload(1)
        }
    } else {
        // edit
        const successful = await settingsStore.updateCustomSensor(customSensor)
        if (successful) {
            if (sensorName.value) {
                sensorName.value = deviceStore.sanitizeString(sensorName.value)
                deviceSettings.sensorsAndChannels.get(customSensor.id as string)!.userName =
                    sensorName.value
            } else {
                // reset name
                deviceSettings.sensorsAndChannels.get(customSensor.id as string)!.userName =
                    undefined
            }
            await deviceStore.waitAndReload(1)
        }
    }
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

const changeSensorType = (event: ListboxChangeEvent): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    selectedSensorType.value = event.value
}
const changeMixFunction = (event: ListboxChangeEvent): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    selectedMixFunction.value = event.value
}

const createNewDashboard = (): Dashboard => {
    const dash = new Dashboard(customSensor.id as string)
    dash.timeRangeSeconds = 300
    dash.deviceChannelNames.push(
        new DashboardDeviceChannel(customSensorsDeviceUID, customSensor.id as string),
    )
    if (deviceSettings.sensorsAndChannels.has(customSensor.id as string)) {
        deviceSettings.sensorsAndChannels.get(customSensor.id as string)!.channelDashboard = dash
    }
    return dash
}
const singleDashboard = ref(
    deviceSettings.sensorsAndChannels.get(customSensor.id as string)?.channelDashboard ??
        createNewDashboard(),
)
const chartTypes = [...$enum(ChartType).values()]
const chartMinutesMin: number = 1
const chartMinutesMax: number = 60
const chartMinutes: Ref<number> = ref(singleDashboard.value.timeRangeSeconds / 60)
const chartMinutesScrolled = (event: WheelEvent): void => {
    if (event.deltaY < 0) {
        if (chartMinutes.value < chartMinutesMax) chartMinutes.value += 1
    } else {
        if (chartMinutes.value > chartMinutesMin) chartMinutes.value -= 1
    }
}

const addScrollEventListener = (): void => {
    // @ts-ignore
    document?.querySelector('.chart-minutes')?.addEventListener('wheel', chartMinutesScrolled)
}
const chartMinutesChanged = (value: number): void => {
    singleDashboard.value.timeRangeSeconds = value * 60
}
const chartKey: Ref<string> = ref(uuidV4())
// const inputArea = ref()
// nextTick(async () => {
//     const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
//     await delay()
//     inputArea.value.$el.focus()
// })

const checkForUnsavedChanges = (_to: any, _from: any, next: any): void => {
    if (!contextIsDirty) {
        next()
        return
    }
    confirm.require({
        message: 'There are unsaved changes made to this Custom Sensor.',
        header: 'Unsaved Changes',
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        rejectLabel: 'Stay',
        acceptLabel: 'Discard',
        accept: () => {
            next()
            contextIsDirty = false
        },
        reject: () => next(false),
    })
}
const viewTypeChanged = () =>
    (deviceSettings.sensorsAndChannels.get(customSensor.id as string)!.viewType =
        chosenViewType.value)

onMounted(async () => {
    watch(currentDeviceStatus, () => {
        updateTemps()
    })
    watch(settingsStore.allUIDeviceSettings, async () => {
        await fillTempSources()
        fillChosenTempSources()
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true })()
    })
    watch([selectedSensorType, selectedMixFunction, filePath, chosenTempSources], () => {
        contextIsDirty = true
    })
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)

    addScrollEventListener()
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
    })
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl">
            {{ shouldCreateSensor ? `New Sensor: ${currentName}` : currentName }}
        </div>
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div
                v-if="
                    chosenViewType === ChannelViewType.Dashboard &&
                    singleDashboard.chartType == ChartType.TIME_CHART
                "
                class="p-2 flex flex-row"
            >
                <InputNumber
                    placeholder="Minutes"
                    input-id="chart-minutes"
                    v-model="chartMinutes"
                    class="h-[2.375rem] chart-minutes"
                    suffix=" min"
                    show-buttons
                    :use-grouping="false"
                    :step="1"
                    :min="chartMinutesMin"
                    :max="chartMinutesMax"
                    button-layout="horizontal"
                    :allow-empty="false"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.bottom="'Time Range'"
                >
                    <template #incrementicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
                <axis-options class="h-[2.375rem] ml-3" :dashboard="singleDashboard" />
            </div>
            <div
                v-if="chosenViewType === ChannelViewType.Dashboard"
                class="p-2"
            >
                <Select
                    v-model="singleDashboard.chartType"
                    :options="chartTypes"
                    placeholder="Select a Chart Type"
                    class="w-32 h-full"
                    checkmark
                    dropdown-icon="pi pi-chart-bar"
                    scroll-height="400px"
                    v-tooltip.bottom="'Chart Type'"
                />
            </div>
            <div
                v-if="!shouldCreateSensor"
                class="p-2"
            >
                <Select
                    v-model="chosenViewType"
                    :options="viewTypeOptions"
                    class="w-32 h-[2.375rem]"
                    placeholder="View Type"
                    checkmark
                    dropdown-icon="pi pi-sliders-h"
                    scroll-height="40rem"
                    @change="viewTypeChanged"
                    v-tooltip.bottom="'Control this channel or view its dashboard.'"
                />
            </div>
            <div class="p-2">
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    label="Save"
                    v-tooltip.bottom="'Save Sensor'"
                    :disabled="chosenViewType !== ChannelViewType.Control"
                    @click="saveSensor"
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
    </div>
    <ScrollAreaRoot
        v-if="chosenViewType === ChannelViewType.Control"
        style="--scrollbar-size: 10px"
    >
        <ScrollAreaViewport class="p-4 pb-16 h-screen w-full">
            <!--            <small class="mt-8 ml-3 font-light text-sm text-text-color-secondary">-->
            <!--                Sensor Name-->
            <!--            </small>-->
            <!--            <div class="mt-1">-->
            <!--                <InputText-->
            <!--                    ref="inputArea"-->
            <!--                    id="name"-->
            <!--                    v-model="sensorName"-->
            <!--                    class="w-96"-->
            <!--                    @keydown.enter="saveSensor"-->
            <!--                    :placeholder="sensorID"-->
            <!--                    v-tooltip.right="'Sensor Name'"-->
            <!--                />-->
            <!--            </div>-->
            <!--            <small class="ml-2 mb-4 font-light text-xs" id="rename-help">-->
            <!--                A blank name will use the system default.-->
            <!--            </small>-->
            <div class="w-full flex flex-col lg:flex-row">
                <div class="mt-0 mr-4 w-96">
                    <small class="ml-3 font-light text-sm text-text-color-secondary">
                        Sensor Type
                    </small>
                    <Listbox
                        :model-value="selectedSensorType"
                        :options="sensorTypes"
                        class="w-full"
                        checkmark
                        placeholder="Type"
                        list-style="max-height: 100%"
                        v-tooltip.right="'Sensor Type'"
                        @change="changeSensorType"
                    />
                </div>
                <div v-if="selectedSensorType === CustomSensorType.Mix" class="mt-0 w-96">
                    <small class="ml-3 font-light text-sm text-text-color-secondary">
                        Mix Function
                    </small>
                    <Listbox
                        :model-value="selectedMixFunction"
                        :options="mixFunctions"
                        checkmark
                        placeholder="Type"
                        class="w-full"
                        list-style="max-height: 100%"
                        v-tooltip.right="'How to calculate the resulting sensor value'"
                        @change="changeMixFunction"
                    />
                </div>
                <div
                    v-else-if="selectedSensorType === CustomSensorType.File"
                    class="flex flex-col w-96 mt-5"
                >
                    <small class="ml-3 font-light text-sm text-text-color-secondary">
                        Temp File Location
                    </small>
                    <InputText
                        v-model="filePath"
                        class="w-full"
                        placeholder="/tmp/your_temp_file"
                        :invalid="!filePath"
                        v-tooltip.bottom="
                            'Enter the absolute path to the temperature file to use for this ' +
                            'sensor.\nThe file must use the sysfs data format standard:\n' +
                            'A fixed point number in millidegrees Celsius.\n' +
                            'e.g. 80000 for 80°C.\n' +
                            'The file is verified upon submission.'
                        "
                    />
                </div>
            </div>
            <div
                v-if="selectedSensorType === CustomSensorType.Mix"
                class="flex flex-col lg:flex-row mt-0 w-full"
            >
                <div class="w-96 mr-4">
                    <small class="ml-3 font-light text-sm text-text-color-secondary">
                        Temp Sources
                    </small>
                    <Listbox
                        v-model="chosenTempSources"
                        class="w-full mt-1"
                        :options="tempSources"
                        multiple
                        filter
                        checkmark
                        option-label="tempFrontendName"
                        option-group-label="deviceName"
                        option-group-children="temps"
                        filter-placeholder="Search"
                        list-style="max-height: 100%"
                        :invalid="chosenTempSources.length === 0"
                        v-tooltip.right="'Temperature sources to be used in the mix function'"
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
                    </Listbox>
                </div>
                <div
                    v-if="selectedMixFunction === CustomSensorMixFunctionType.WeightedAvg"
                    class="mt-1 w-96"
                    v-tooltip.right="'The individual weight of each selected temperature source.'"
                >
                    <small class="ml-3 font-light text-sm text-text-color-secondary">
                        Temp Weights
                    </small>
                    <DataTable :value="chosenTempSources">
                        <Column field="tempFrontendName" header="Temp Name" body-class="w-full">
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
        </ScrollAreaViewport>
        <ScrollAreaScrollbar
            class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
            orientation="vertical"
        >
            <ScrollAreaThumb
                class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
            />
        </ScrollAreaScrollbar>
    </ScrollAreaRoot>
    <div v-else-if="chosenViewType === ChannelViewType.Dashboard">
        <TimeChart
            v-if="singleDashboard.chartType == ChartType.TIME_CHART"
            :dashboard="singleDashboard"
            :key="chartKey"
        />
        <SensorTable
            v-else-if="singleDashboard.chartType == ChartType.TABLE"
            :dashboard="singleDashboard"
            :key="'table' + chartKey"
        />
    </div>
</template>

<style scoped lang="scss"></style>
