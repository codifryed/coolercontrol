<!--
  SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiAlertCircle,
    mdiAlertOutline,
    mdiChartLine,
    mdiContentSaveOutline,
    mdiFolderSearchOutline,
    mdiMinusThick,
    mdiTrashCanOutline,
} from '@mdi/js'
import {
    CustomSensor,
    CustomSensorMixFunctionType,
    CustomSensorTempSource,
    CustomSensorType,
    CustomTempSourceData,
    getCustomSensorTypeDisplayName,
    getCustomSensorMixFunctionTypeDisplayName,
} from '@/models/CustomSensor.ts'
import { onMounted, ref, toRaw, type Ref, watch, computed } from 'vue'
import { $enum } from 'ts-enum-util'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { DeviceType, UID } from '@/models/Device.ts'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'reka-ui'
import { onBeforeRouteLeave, onBeforeRouteUpdate, RouterLink } from 'vue-router'
import { useConfirm } from '@/shell/confirm'
import UiListbox from '@/shell/ui/UiListbox.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import UiInput from '@/shell/ui/UiInput.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiTable from '@/shell/ui/UiTable.vue'
import UiGroupedListbox from '@/shell/ui/UiGroupedListbox.vue'
import { Dashboard, DashboardDeviceChannel } from '@/models/Dashboard.ts'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'
import { useI18n } from 'vue-i18n'
import EntityTitleRename from '@/components/EntityTitleRename.vue'
import EntityPageHeader from '@/components/EntityPageHeader.vue'
import HealthWarning from '@/components/HealthWarning.vue'
import TimeChart from '@/components/TimeChart.vue'

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
// We need to use the raw state to watch for changes, as the pinia reactive proxy isn't properly
// reacting to changes from Vue's shallowRef & triggerRef anymore.
const rawStore = toRaw(deviceStore.$state)
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const { t } = useI18n()

const contextIsDirty: Ref<boolean> = ref(false)
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

const customSensors: Array<CustomSensor> = await settingsStore.getCustomSensors()
const collectCustomSensor = async (): Promise<CustomSensor> => {
    if (shouldCreateSensor) {
        const newSensorNumber =
            customSensorIdNumbers.length === 0
                ? 1
                : customSensorIdNumbers[customSensorIdNumbers.length - 1] + 1
        return new CustomSensor(`sensor${newSensorNumber}`)
    } else {
        const foundSensor = customSensors.find((cs) => cs.id === props.customSensorID)
        if (foundSensor == undefined) {
            throw new Error(
                `Illegal State: Could not find Custom Sensor with ID: ${props.customSensorID} in ${customSensors}`,
            )
        }
        return foundSensor
    }
}
const customSensor: CustomSensor = await collectCustomSensor()

// @ts-ignore
const sensorID: Ref<string> = ref(customSensor.id)
const currentName: Ref<string> = ref(
    deviceSettings.sensorsAndChannels.get(customSensor.id)?.name ?? sensorID.value,
)
const isUserName: boolean =
    settingsStore.nameOverrides.devices[customSensorsDeviceUID]?.channels?.[customSensor.id]
        ?.label != undefined
// .value, not the ref: ref(existingRef) returns that same ref, which would alias
// this to currentName and let saveNameFunction's second write undo its first.
const sensorName: Ref<string> = ref(isUserName ? currentName.value : '')
const selectedSensorType: Ref<CustomSensorType> = ref(customSensor.cs_type)
const selectedMixFunction: Ref<CustomSensorMixFunctionType> = ref(customSensor.mix_function)
const selectedOffset: Ref<number> = ref(customSensor.offset ?? 0)
const selectedTimeWindowSeconds: Ref<number> = ref(customSensor.time_window_seconds ?? 10)

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

const getSensorTypeHelpText = (type: CustomSensorType): string => {
    switch (type) {
        case CustomSensorType.Mix:
            return t('views.customSensors.helpText.mix')
        case CustomSensorType.File:
            return t('views.customSensors.helpText.file')
        case CustomSensorType.Offset:
            return t('views.customSensors.helpText.offset')
        case CustomSensorType.TimeAverage:
            return t('views.customSensors.helpText.timeAverage')
        case CustomSensorType.ExponentialMovingAvg:
            return t('views.customSensors.helpText.exponentialMovingAvg')
        default:
            return ''
    }
}

const chosenTempSources: Ref<Array<AvailableTemp>> = ref([])
const chosenOffsetTempSource: Ref<AvailableTemp | undefined> = ref(undefined)
const chosenTimeAverageTempSource: Ref<AvailableTemp | undefined> = ref(undefined)
const chosenEmaTempSource: Ref<AvailableTemp | undefined> = ref(undefined)
const filePath: Ref<string> = ref(customSensor.file_path ?? '')

const tempSources: Ref<Array<AvailableTempSources>> = ref([])
const fillTempSources = async (): Promise<void> => {
    tempSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.status.temps.length === 0 || device.info == undefined) {
            continue
        }
        if (device.type === DeviceType.CUSTOM_SENSORS && customSensor.parents.length > 0) {
            // skip custom sensors if it has parents/is a child - it can not also be a parent
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
            if (device.type === DeviceType.CUSTOM_SENSORS) {
                if (temp.name === customSensor.id) {
                    // Cannot have itself as a temp source
                    continue
                }
                const associatedCustomSensor = customSensors.find((cs) => cs.id === temp.name)
                if (associatedCustomSensor == null) {
                    console.error('Could not find associated Custom Sensor by: ', temp.name)
                    continue
                } else if (associatedCustomSensor.children.length > 0) {
                    // If the 'potential child' custom sensor IS a parent/HAS children = do NOT show
                    continue
                }
            }
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
    if (selectedSensorType.value !== CustomSensorType.Mix) {
        return
    }
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

// Sources in the saved config whose target temp is gone. They are invisible in the
// pickers below and will be dropped on save.
const droppedSources: Array<string> = customSensor.sources
    .filter(
        (sourceData) =>
            !tempSources.value.some(
                (device) =>
                    device.deviceUID === sourceData.temp_source.device_uid &&
                    device.temps.some((temp) => temp.tempName === sourceData.temp_source.temp_name),
            ),
    )
    .map((sourceData) => sourceData.temp_source.temp_name)

const fillChosenOffsetTempSource = () => {
    chosenOffsetTempSource.value = undefined
    if (selectedSensorType.value !== CustomSensorType.Offset) {
        return
    }
    for (const customTempSourceData of customSensor.sources) {
        for (const availableTempSource of tempSources.value) {
            if (availableTempSource.deviceUID === customTempSourceData.temp_source.device_uid) {
                for (const availableTemp of availableTempSource.temps) {
                    if (availableTemp.tempName === customTempSourceData.temp_source.temp_name) {
                        availableTemp.weight = customTempSourceData.weight
                        chosenOffsetTempSource.value = availableTemp
                    }
                }
            }
        }
    }
}
fillChosenOffsetTempSource()

const fillChosenTimeAverageTempSource = () => {
    chosenTimeAverageTempSource.value = undefined
    if (selectedSensorType.value !== CustomSensorType.TimeAverage) {
        return
    }
    for (const customTempSourceData of customSensor.sources) {
        for (const availableTempSource of tempSources.value) {
            if (availableTempSource.deviceUID === customTempSourceData.temp_source.device_uid) {
                for (const availableTemp of availableTempSource.temps) {
                    if (availableTemp.tempName === customTempSourceData.temp_source.temp_name) {
                        availableTemp.weight = customTempSourceData.weight
                        chosenTimeAverageTempSource.value = availableTemp
                    }
                }
            }
        }
    }
}
fillChosenTimeAverageTempSource()

const fillChosenEmaTempSource = () => {
    chosenEmaTempSource.value = undefined
    if (selectedSensorType.value !== CustomSensorType.ExponentialMovingAvg) {
        return
    }
    for (const customTempSourceData of customSensor.sources) {
        for (const availableTempSource of tempSources.value) {
            if (availableTempSource.deviceUID === customTempSourceData.temp_source.device_uid) {
                for (const availableTemp of availableTempSource.temps) {
                    if (availableTemp.tempName === customTempSourceData.temp_source.temp_name) {
                        availableTemp.weight = customTempSourceData.weight
                        chosenEmaTempSource.value = availableTemp
                    }
                }
            }
        }
    }
}
fillChosenEmaTempSource()

const saveSensor = async (): Promise<void> => {
    customSensor.cs_type = selectedSensorType.value
    customSensor.mix_function = selectedMixFunction.value
    const tempSources: Array<CustomTempSourceData> = []
    if (customSensor.cs_type === CustomSensorType.File) {
        customSensor.offset = undefined
        customSensor.time_window_seconds = undefined
        customSensor.file_path = filePath.value
    } else if (customSensor.cs_type === CustomSensorType.Mix) {
        if (chosenTempSources.value == null || chosenTempSources.value.length === 0) {
            console.error('No temp sources selected')
            return
        }
        customSensor.file_path = undefined
        customSensor.offset = undefined
        customSensor.time_window_seconds = undefined
        chosenTempSources.value.forEach((tempSource) =>
            tempSources.push(
                new CustomTempSourceData(
                    new CustomSensorTempSource(tempSource.deviceUID, tempSource.tempName),
                    tempSource.weight,
                ),
            ),
        )
    } else if (customSensor.cs_type === CustomSensorType.Offset) {
        if (chosenOffsetTempSource.value == null) {
            console.error('No offset temp source selected')
            return
        }
        customSensor.file_path = undefined
        customSensor.offset = selectedOffset.value
        customSensor.time_window_seconds = undefined
        tempSources.push(
            new CustomTempSourceData(
                new CustomSensorTempSource(
                    chosenOffsetTempSource.value.deviceUID,
                    chosenOffsetTempSource.value.tempName,
                ),
                chosenOffsetTempSource.value.weight,
            ),
        )
    } else if (customSensor.cs_type === CustomSensorType.TimeAverage) {
        if (chosenTimeAverageTempSource.value == null) {
            console.error('No time-average temp source selected')
            return
        }
        customSensor.file_path = undefined
        customSensor.offset = undefined
        customSensor.time_window_seconds = selectedTimeWindowSeconds.value
        tempSources.push(
            new CustomTempSourceData(
                new CustomSensorTempSource(
                    chosenTimeAverageTempSource.value.deviceUID,
                    chosenTimeAverageTempSource.value.tempName,
                ),
                chosenTimeAverageTempSource.value.weight,
            ),
        )
    } else if (customSensor.cs_type === CustomSensorType.ExponentialMovingAvg) {
        if (chosenEmaTempSource.value == null) {
            console.error('No EMA temp source selected')
            return
        }
        customSensor.file_path = undefined
        customSensor.offset = undefined
        customSensor.time_window_seconds = selectedTimeWindowSeconds.value
        tempSources.push(
            new CustomTempSourceData(
                new CustomSensorTempSource(
                    chosenEmaTempSource.value.deviceUID,
                    chosenEmaTempSource.value.tempName,
                ),
                chosenEmaTempSource.value.weight,
            ),
        )
    }
    customSensor.sources = tempSources

    if (shouldCreateSensor) {
        const successful = await settingsStore.saveCustomSensor(customSensor)
        if (successful) {
            // The name is a daemon override on the new sensor's channel;
            // saveChannelName surfaces a rejected name as a toast.
            if (sensorName.value) {
                sensorName.value = deviceStore.sanitizeString(sensorName.value)
                await settingsStore.saveChannelName(
                    customSensorsDeviceUID,
                    customSensor.id,
                    sensorName.value,
                )
            }
            await deviceStore.waitAndReload(1)
        }
    } else {
        // edit
        const successful = await settingsStore.updateCustomSensor(customSensor)
        if (successful) {
            if (sensorName.value) {
                sensorName.value = deviceStore.sanitizeString(sensorName.value)
            }
            await settingsStore.saveChannelName(
                customSensorsDeviceUID,
                customSensor.id,
                sensorName.value,
            )
            await deviceStore.waitAndReload(1)
        }
    }
}
const defaultLabel = computed(() =>
    settingsStore.defaultChannelLabel(customSensorsDeviceUID, customSensor.id),
)
const saveNameFunction = async (newName: string): Promise<boolean> => {
    // User names are persisted as daemon name overrides. An empty name removes
    // the override and falls back to the detected label, which has to be read
    // before saving drops the override it is derived from.
    const applied = newName.length > 0 ? newName : defaultLabel.value
    const success = await settingsStore.saveChannelName(
        customSensorsDeviceUID,
        customSensor.id,
        newName,
    )
    if (!success) {
        return false
    }
    sensorName.value = newName.length > 0 ? newName : ''
    currentName.value = applied
    return true
}
const deleteSensor = (): void => {
    confirm.require({
        message: t('views.customSensors.deleteCustomSensorConfirm', { name: currentName.value }),
        header: t('views.customSensors.deleteCustomSensor'),
        icon: mdiAlertOutline,
        accept: async () => {
            contextIsDirty.value = false
            await settingsStore.deleteCustomSensor(customSensorsDeviceUID, customSensor.id)
        },
    })
}
const updateTemps = () => {
    for (const tempDevice of tempSources.value) {
        for (const availableTemp of tempDevice.temps) {
            availableTemp.temp =
                deviceStore.currentDeviceStatus
                    .get(availableTemp.deviceUID)!
                    .get(availableTemp.tempName)!.temp || '0.0'
        }
    }
}

const changeSensorType = (value: string | undefined): void => {
    if (value == null) {
        return // do not update on unselect
    }
    selectedSensorType.value = value as CustomSensorType
}
const changeMixFunction = (value: string | undefined): void => {
    if (value == null) {
        return // do not update on unselect
    }
    selectedMixFunction.value = value as CustomSensorMixFunctionType
}

const tempKey = (temp: AvailableTemp): string => `${temp.deviceUID}/${temp.tempName}`
const allTemps = computed(() => tempSources.value.flatMap((source) => source.temps))
const tempGroups = computed(() =>
    tempSources.value.map((source) => ({
        label: source.deviceName,
        options: source.temps.map((temp) => ({
            label: temp.tempFrontendName,
            value: tempKey(temp),
            color: temp.lineColor,
            rightText: `${temp.temp} ${t('common.tempUnit')}`,
        })),
    })),
)
const findTemp = (key: string | string[] | undefined): AvailableTemp | undefined =>
    typeof key === 'string'
        ? allTemps.value.find((candidate) => tempKey(candidate) === key)
        : undefined
const chosenTempSourceKeys = computed<string[] | string | undefined>({
    get: () => chosenTempSources.value.map(tempKey),
    set: (keys) => {
        if (!Array.isArray(keys)) return
        chosenTempSources.value = keys
            .map((key) => findTemp(key))
            .filter((temp): temp is AvailableTemp => temp != null)
    },
})
const singleTempKeyModel = (source: Ref<AvailableTemp | undefined>) =>
    computed<string | string[] | undefined>({
        get: () => (source.value != null ? tempKey(source.value) : undefined),
        set: (key) => {
            const temp = findTemp(key)
            if (temp != null) source.value = temp
        },
    })
const chosenOffsetTempSourceKey = singleTempKeyModel(chosenOffsetTempSource)
const chosenTimeAverageTempSourceKey = singleTempKeyModel(chosenTimeAverageTempSource)
const chosenEmaTempSourceKey = singleTempKeyModel(chosenEmaTempSource)

const createNewDashboard = (): Dashboard => {
    const dash = new Dashboard(customSensor.id)
    dash.timeRangeSeconds = 300
    dash.deviceChannelNames.push(
        new DashboardDeviceChannel(customSensorsDeviceUID, customSensor.id),
    )
    if (deviceSettings.sensorsAndChannels.has(customSensor.id)) {
        deviceSettings.sensorsAndChannels.get(customSensor.id)!.channelDashboard = dash
    }
    return dash
}
const singleDashboard = ref(
    deviceSettings.sensorsAndChannels.get(customSensor.id)?.channelDashboard ??
        createNewDashboard(),
)
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

// The chart canvas consumes plain wheel events for zoom; stop them in the
// capture phase so the page keeps scrolling. Ctrl+wheel still zooms.
const onChartWheelCapture = (event: WheelEvent): void => {
    if (!event.ctrlKey) event.stopPropagation()
}
// const inputArea = ref()
// nextTick(async () => {
//     const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
//     await delay()
//     inputArea.value.$el.focus()
// })

const checkForUnsavedChanges = (): boolean | Promise<boolean> => {
    if (!contextIsDirty.value) {
        return true
    }
    return new Promise<boolean>((resolve) => {
        confirm.require({
            message: t('views.customSensors.unsavedChanges'),
            header: t('views.customSensors.unsavedChangesHeader'),
            icon: mdiAlertOutline,
            defaultFocus: 'accept',
            rejectLabel: t('common.stay'),
            acceptLabel: t('common.discard'),
            accept: () => {
                contextIsDirty.value = false
                resolve(true)
            },
            reject: () => resolve(false),
        })
    })
}

const fileBrowse = async (): Promise<void> => {
    // @ts-ignore
    const ipc = window.ipc
    filePath.value = await ipc.filePathDialog(t('views.customSensors.selectCustomSensorFile'))
}

const saveButtonDisabled = (): boolean => {
    return (
        (selectedSensorType.value === CustomSensorType.Mix &&
            chosenTempSources.value.length === 0) ||
        (selectedSensorType.value === CustomSensorType.Offset &&
            chosenOffsetTempSource.value == null) ||
        (selectedSensorType.value === CustomSensorType.File && filePath.value === null) ||
        (selectedSensorType.value === CustomSensorType.TimeAverage &&
            (chosenTimeAverageTempSource.value == null ||
                selectedTimeWindowSeconds.value == null ||
                selectedTimeWindowSeconds.value < 1 ||
                selectedTimeWindowSeconds.value > 300)) ||
        (selectedSensorType.value === CustomSensorType.ExponentialMovingAvg &&
            (chosenEmaTempSource.value == null ||
                selectedTimeWindowSeconds.value == null ||
                selectedTimeWindowSeconds.value < 1 ||
                selectedTimeWindowSeconds.value > 300))
    )
}

onMounted(async () => {
    watch(rawStore.currentDeviceStatus, () => {
        updateTemps()
    })
    watch(settingsStore.allUIDeviceSettings, async () => {
        await fillTempSources()
        fillChosenTempSources()
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true })()
    })
    watch(
        [
            selectedSensorType,
            selectedMixFunction,
            filePath,
            chosenTempSources,
            selectedOffset,
            chosenOffsetTempSource,
            selectedTimeWindowSeconds,
            chosenTimeAverageTempSource,
            chosenEmaTempSource,
        ],
        () => {
            contextIsDirty.value = true
        },
    )
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)

    addScrollEventListener()
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
    })
})
</script>

<template>
    <div class="flex h-full flex-col">
        <entity-page-header>
            <template #title>
                <entity-title-rename
                    :current-name="currentName"
                    :fallback-name="defaultLabel"
                    :save-name-function="saveNameFunction"
                />
            </template>
            <template #actions>
                <UiButton
                    v-if="!shouldCreateSensor"
                    variant="ghost"
                    size="icon"
                    v-tooltip.top="t('views.customSensors.deleteCustomSensor')"
                    @click="deleteSensor"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiTrashCanOutline"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                </UiButton>

                <div class="p-2">
                    <UiButton
                        class="w-32"
                        v-tooltip.top="t('views.customSensors.saveCustomSensor')"
                        :disabled="saveButtonDisabled()"
                        @click="saveSensor"
                    >
                        <svg-icon
                            class="outline-0"
                            type="mdi"
                            :path="mdiContentSaveOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                    </UiButton>
                </div>
            </template>
        </entity-page-header>
        <ScrollAreaRoot class="min-h-0 flex-1" style="--scrollbar-size: 10px">
            <ScrollAreaViewport class="p-4 h-full w-full">
                <health-warning
                    kind="custom-sensor"
                    :entity-uid="props.customSensorID"
                    class="mb-4"
                />
                <div
                    v-if="droppedSources.length > 0"
                    class="mb-4 flex flex-row items-center gap-2 rounded-lg border border-warning bg-warning/10 p-3"
                >
                    <svg-icon
                        type="mdi"
                        class="text-warning min-w-6"
                        :path="mdiAlertCircle"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                    <span>
                        {{
                            t('views.customSensors.missingSourcesNotice', {
                                sources: droppedSources.join(', '),
                            })
                        }}
                    </span>
                </div>
                <div class="w-full flex flex-col lg:flex-row">
                    <div class="mt-0 lg:mr-4 w-full max-w-96">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.sensorType') }}
                        </small>
                        <UiListbox
                            :model-value="selectedSensorType"
                            :options="sensorTypeOptions"
                            class="w-full"
                            @update:model-value="changeSensorType"
                        >
                            <template #option="{ option }">
                                <div
                                    class="w-full"
                                    v-tooltip.right="
                                        getSensorTypeHelpText(option.value as CustomSensorType)
                                    "
                                >
                                    {{ option.label }}
                                </div>
                            </template>
                        </UiListbox>
                    </div>
                    <div
                        v-if="selectedSensorType === CustomSensorType.Mix"
                        class="mt-4 lg:mt-0 w-full max-w-96"
                    >
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.mixFunction') }}
                        </small>
                        <UiListbox
                            :model-value="selectedMixFunction"
                            :options="mixFunctionTypeOptions"
                            class="w-full"
                            v-tooltip.top="t('views.customSensors.howCalculateValue')"
                            @update:model-value="changeMixFunction"
                        />
                    </div>
                    <div
                        v-if="selectedSensorType === CustomSensorType.Offset"
                        class="flex flex-col mt-1 w-full max-w-96 mb-28"
                    >
                        <small class="ml-3 mb-1 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.offset') }}
                        </small>
                        <div
                            class="rounded-lg bg-bg-two p-3 flex justify-center"
                            v-tooltip.top="{
                                escape: false,
                                value: t('views.customSensors.offsetTooltip'),
                            }"
                        >
                            <UiNumberInput v-model="selectedOffset" :min="-100" :max="100" />
                        </div>
                    </div>
                    <div
                        v-if="
                            selectedSensorType === CustomSensorType.TimeAverage ||
                            selectedSensorType === CustomSensorType.ExponentialMovingAvg
                        "
                        class="flex flex-col mt-1 w-full max-w-96 mb-28"
                    >
                        <small class="ml-3 mb-1 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.timeWindow') }}
                        </small>
                        <div
                            class="rounded-lg bg-bg-two p-3 flex justify-center"
                            v-tooltip.top="{
                                escape: false,
                                value: t('views.customSensors.timeWindowTooltip'),
                            }"
                        >
                            <UiNumberInput
                                v-model="selectedTimeWindowSeconds"
                                :min="1"
                                :max="300"
                                :suffix="t('common.secondAbbr')"
                            />
                        </div>
                    </div>
                    <div
                        v-else-if="selectedSensorType === CustomSensorType.File"
                        class="flex flex-col w-full max-w-96 mt-1"
                    >
                        <small class="ml-3 mb-1 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.tempFile') }}
                        </small>
                        <UiInput
                            v-model="filePath"
                            class="w-full"
                            placeholder="/tmp/your_temp_file"
                            :class="{ '!border-error': !filePath }"
                            v-tooltip.top="t('views.customSensors.filePathTooltip')"
                        />
                        <div v-if="deviceStore.isQtApp()">
                            <UiButton
                                class="mt-2 w-full"
                                v-tooltip.top="t('views.customSensors.browseCustomSensorFile')"
                                @click="fileBrowse"
                            >
                                <svg-icon
                                    class="outline-0 mt-[-0.25rem]"
                                    type="mdi"
                                    :path="mdiFolderSearchOutline"
                                    :size="deviceStore.getREMSize(1.5)"
                                />
                                {{ t('views.customSensors.browse') }}
                            </UiButton>
                        </div>
                    </div>
                </div>
                <div
                    v-if="selectedSensorType === CustomSensorType.Mix"
                    class="flex flex-col lg:flex-row mt-0 w-full"
                >
                    <div class="w-full max-w-xl lg:mr-4">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.tempSources') }}
                        </small>
                        <UiGroupedListbox
                            v-model="chosenTempSourceKeys"
                            class="w-full max-h-[28rem]"
                            :groups="tempGroups"
                            filter
                            :filter-placeholder="t('common.search')"
                            multiple
                            :invalid="chosenTempSources == null || chosenTempSources.length === 0"
                            v-tooltip.top="{
                                escape: false,
                                value: t('views.customSensors.tempSourcesTooltip'),
                            }"
                        />
                    </div>
                    <div
                        v-if="selectedMixFunction === CustomSensorMixFunctionType.WeightedAvg"
                        class="w-full max-w-xl mt-4 lg:mt-0"
                        v-tooltip.top="t('views.customSensors.tempWeights')"
                    >
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.tempWeights') }}
                        </small>
                        <UiTable bordered>
                            <template #head>
                                <tr>
                                    <th class="w-full">{{ t('views.customSensors.tempName') }}</th>
                                    <th>{{ t('views.customSensors.weight') }}</th>
                                </tr>
                            </template>
                            <tr
                                v-for="source in chosenTempSources"
                                :key="`${source.deviceUID}/${source.tempName}`"
                            >
                                <td>
                                    <div class="flex items-center gap-2">
                                        <svg-icon
                                            type="mdi"
                                            :path="mdiMinusThick"
                                            :size="14"
                                            class="shrink-0"
                                            :style="{ color: source.lineColor }"
                                        />
                                        {{ source.tempFrontendName }}
                                    </div>
                                </td>
                                <td>
                                    <UiNumberInput v-model="source.weight" :min="1" :max="254" />
                                </td>
                            </tr>
                        </UiTable>
                    </div>
                </div>
                <!--Need a separate model for single-selection temp source-->
                <div
                    v-if="selectedSensorType === CustomSensorType.Offset"
                    class="flex flex-col lg:flex-row mt-0 w-full"
                >
                    <div class="w-full max-w-xl lg:mr-4">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.tempSource') }}
                        </small>
                        <UiGroupedListbox
                            v-model="chosenOffsetTempSourceKey"
                            class="w-full mt-1 max-h-[28rem]"
                            :groups="tempGroups"
                            filter
                            :filter-placeholder="t('common.search')"
                            :invalid="chosenOffsetTempSource == null"
                        />
                    </div>
                </div>
                <div
                    v-if="selectedSensorType === CustomSensorType.TimeAverage"
                    class="flex flex-col lg:flex-row mt-0 w-full"
                >
                    <div class="w-full max-w-xl lg:mr-4">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.tempSource') }}
                        </small>
                        <UiGroupedListbox
                            v-model="chosenTimeAverageTempSourceKey"
                            class="w-full mt-1 max-h-[28rem]"
                            :groups="tempGroups"
                            filter
                            :filter-placeholder="t('common.search')"
                            :invalid="chosenTimeAverageTempSource == null"
                        />
                    </div>
                </div>
                <div
                    v-if="selectedSensorType === CustomSensorType.ExponentialMovingAvg"
                    class="flex flex-col lg:flex-row mt-0 w-full"
                >
                    <div class="w-full max-w-xl lg:mr-4">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.tempSource') }}
                        </small>
                        <UiGroupedListbox
                            v-model="chosenEmaTempSourceKey"
                            class="w-full mt-1 max-h-[28rem]"
                            :groups="tempGroups"
                            filter
                            :filter-placeholder="t('common.search')"
                            :invalid="chosenEmaTempSource == null"
                        />
                    </div>
                </div>
                <div
                    v-if="!shouldCreateSensor"
                    class="mt-6 shrink-0"
                    style="--time-chart-height: 24rem"
                    @wheel.capture="onChartWheelCapture"
                >
                    <div class="mb-1 flex justify-center">
                        <RouterLink
                            :to="{
                                name: 'monitoring-sensor',
                                params: {
                                    deviceUID: customSensorsDeviceUID,
                                    channelName: customSensor.id,
                                },
                            }"
                            class="flex items-center gap-1 rounded-lg px-2 py-1 text-sm text-text-color-secondary outline-none hover:bg-surface-hover hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                        >
                            <svg-icon type="mdi" :path="mdiChartLine" :size="16" />
                            {{ t('layout.shell.coolingPage.fullChart') }}
                        </RouterLink>
                    </div>
                    <TimeChart
                        :key="chartKey"
                        :dashboard="singleDashboard"
                        @line-set-changed="chartKey = uuidV4()"
                    />
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
    </div>
</template>

<style scoped lang="scss"></style>
