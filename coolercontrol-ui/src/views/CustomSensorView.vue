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
    CustomSensorMetric,
    CustomSensorMixFunctionType,
    CustomSensorSourceData,
    CustomSensorType,
    getCustomSensorMetricDisplayName,
    getCustomSensorTypeDisplayName,
    getCustomSensorMixFunctionTypeDisplayName,
} from '@/models/CustomSensor.ts'
import { onMounted, ref, toRaw, type Ref, watch, computed } from 'vue'
import { $enum } from 'ts-enum-util'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { DeviceType, UID } from '@/models/Device.ts'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'reka-ui'
import {
    onBeforeRouteLeave,
    onBeforeRouteUpdate,
    type RouteLocationRaw,
    RouterLink,
    useRouter,
} from 'vue-router'
import { useConfirm } from '@/shell/confirm'
import UiListbox from '@/shell/ui/UiListbox.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import UiInput from '@/shell/ui/UiInput.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiSelect from '@/shell/ui/UiSelect.vue'
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
import {
    type AvailableSource,
    type AvailableSourceGroup,
    availableSources,
    canSave,
    liveValue,
    newCustomSensorId,
    offsetLimit,
    SCALE_MAGNITUDE_MAX,
    TIME_WINDOW_SECONDS_MAX,
    TIME_WINDOW_SECONDS_MIN,
} from '@/components/customSensorEditor.ts'

interface Props {
    customSensorID?: string
}

const props = defineProps<Props>()
const deviceStore = useDeviceStore()
// We need to use the raw state to watch for changes, as the pinia reactive proxy isn't properly
// reacting to changes from Vue's shallowRef & triggerRef anymore.
const rawStore = toRaw(deviceStore.$state)
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const router = useRouter()
const { t } = useI18n()

const contextIsDirty: Ref<boolean> = ref(false)
const shouldCreateSensor: boolean = !props.customSensorID
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

const customSensors: Array<CustomSensor> = await settingsStore.getCustomSensors()
const collectCustomSensor = async (): Promise<CustomSensor> => {
    if (shouldCreateSensor) {
        return new CustomSensor(newCustomSensorId(new Set(customSensors.map((cs) => cs.id))))
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
const selectedMetric: Ref<CustomSensorMetric> = ref(customSensor.metric)
const selectedSensorType: Ref<CustomSensorType> = ref(customSensor.cs_type)
const selectedMixFunction: Ref<CustomSensorMixFunctionType> = ref(customSensor.mix_function)
const selectedScale: Ref<number> = ref(customSensor.scale ?? 1)
const selectedOffset: Ref<number> = ref(customSensor.offset ?? 0)
const selectedTimeWindowSeconds: Ref<number> = ref(customSensor.time_window_seconds ?? 10)

// Generate options with localized display names
const metricOptions = computed(() => {
    return [...$enum(CustomSensorMetric).values()].map((metric) => ({
        value: metric,
        label: getCustomSensorMetricDisplayName(metric),
    }))
})

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

// Scale & Offset and the two smoothing types read exactly one source.
const isSingleSourceType = (type: CustomSensorType): boolean =>
    type === CustomSensorType.Offset ||
    type === CustomSensorType.TimeAverage ||
    type === CustomSensorType.ExponentialMovingAvg
const isSmoothingType = (type: CustomSensorType): boolean =>
    type === CustomSensorType.TimeAverage || type === CustomSensorType.ExponentialMovingAvg

const chosenMixSources: Ref<Array<AvailableSource>> = ref([])
const chosenSingleSource: Ref<AvailableSource | undefined> = ref(undefined)
const filePath: Ref<string> = ref(customSensor.file_path ?? '')

const sourceKey = (source: { deviceUID: string; name: string }): string =>
    `${source.deviceUID}/${source.name}`
const sourceGroups: Ref<Array<AvailableSourceGroup>> = ref([])
const fillSources = (): void => {
    sourceGroups.value = availableSources(
        deviceStore.allDevices(),
        selectedMetric.value,
        customSensors,
        customSensor,
        (deviceUID) => settingsStore.allUIDeviceSettings.get(deviceUID),
    )
}
fillSources()
const allSources = computed(() => sourceGroups.value.flatMap((group) => group.sources))
const findSource = (key: string | string[] | undefined): AvailableSource | undefined =>
    typeof key === 'string'
        ? allSources.value.find((candidate) => sourceKey(candidate) === key)
        : undefined

// Selects the saved sources among the available ones, with their saved weights.
const selectSavedSources = (): void => {
    const saved: Array<AvailableSource> = []
    for (const sourceData of customSensor.sources) {
        const available = findSource(sourceKey(sourceData))
        if (available == null) continue
        available.weight = sourceData.weight
        saved.push(available)
    }
    const isMix = customSensor.cs_type === CustomSensorType.Mix
    chosenMixSources.value = isMix ? saved : []
    chosenSingleSource.value = isMix ? undefined : saved[0]
}
selectSavedSources()

// Sources in the saved config whose target is gone. They are invisible in the
// pickers below and will be dropped on save.
const droppedSources: Array<string> = customSensor.sources
    .filter((sourceData) => findSource(sourceKey(sourceData)) == null)
    .map((sourceData) => sourceData.name)

// Rebuilds the lists and keeps what is selected: the entries are new objects, so the
// selection is carried over by key.
const refreshSources = (): void => {
    const mixWeights = new Map(
        chosenMixSources.value.map((source) => [sourceKey(source), source.weight]),
    )
    const singleKey =
        chosenSingleSource.value != null ? sourceKey(chosenSingleSource.value) : undefined
    fillSources()
    const mixSources = allSources.value.filter((source) => mixWeights.has(sourceKey(source)))
    for (const source of mixSources) source.weight = mixWeights.get(sourceKey(source))!
    chosenMixSources.value = mixSources
    chosenSingleSource.value = findSource(singleKey)
}

const saveSensor = async (): Promise<void> => {
    if (saveButtonDisabled()) {
        console.error('The Custom Sensor is incomplete')
        return
    }
    const type = selectedSensorType.value
    const metric = selectedMetric.value
    customSensor.metric = metric
    customSensor.cs_type = type
    customSensor.mix_function = selectedMixFunction.value
    customSensor.file_path = type === CustomSensorType.File ? filePath.value : undefined
    const isScaleOffset = type === CustomSensorType.Offset
    customSensor.scale = isScaleOffset ? selectedScale.value : undefined
    customSensor.offset = isScaleOffset ? selectedOffset.value : undefined
    customSensor.time_window_seconds = isSmoothingType(type)
        ? selectedTimeWindowSeconds.value
        : undefined
    let chosenSources: Array<AvailableSource> = []
    if (type === CustomSensorType.Mix) {
        chosenSources = chosenMixSources.value
    } else if (isSingleSourceType(type) && chosenSingleSource.value != null) {
        chosenSources = [chosenSingleSource.value]
    }
    customSensor.sources = chosenSources.map((source) =>
        CustomSensorSourceData.of(metric, source.deviceUID, source.name, source.weight),
    )

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
            // Point the reload at the new sensor, or it lands back on an empty
            // new-sensor form. Not a router navigation: that would set this
            // view up again before the stores know the sensor.
            const created = router.resolve({
                name: 'device-custom-sensor',
                params: { customSensorID: customSensor.id },
            })
            window.history.replaceState(null, '', created.href)
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
    if (shouldCreateSensor) {
        // Nothing to name yet: saveSensor applies it once the sensor exists.
        // Saving it now would leave an override behind if the page is abandoned.
        if (newName === currentName.value) return true
        sensorName.value = newName
        currentName.value = applied
        contextIsDirty.value = true
        return true
    }
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
const updateValues = () => {
    for (const group of sourceGroups.value) {
        for (const source of group.sources) {
            const values = deviceStore.currentDeviceStatus.get(source.deviceUID)?.get(source.name)
            source.value = liveValue(selectedMetric.value, values) ?? source.value
        }
    }
}

const changeMetric = (value: string | undefined): void => {
    if (value == null || value === selectedMetric.value) {
        return // do not update on unselect
    }
    selectedMetric.value = value as CustomSensorMetric
    // A source of the previous metric is no source of this one.
    fillSources()
    chosenMixSources.value = []
    chosenSingleSource.value = undefined
    const limit = offsetLimit(selectedMetric.value)
    selectedOffset.value = Math.min(limit, Math.max(-limit, selectedOffset.value))
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

// The unit a source's value is shown in. An rpm source may name its own.
const sourceUnit = (source: AvailableSource): string => {
    switch (selectedMetric.value) {
        case CustomSensorMetric.Temp:
            return t('common.tempUnit')
        case CustomSensorMetric.Duty:
            return t('common.percentUnit')
        case CustomSensorMetric.RPM:
            return settingsStore.rpmUnit(source.deviceUID, source.name)
        case CustomSensorMetric.Freq:
            return t('common.mhzAbbr')
        case CustomSensorMetric.Watts:
            return t('common.wattAbbr')
        default:
            return ''
    }
}
// What a File sensor of the chosen metric expects in its file.
const fileUnitText = computed((): string => {
    switch (selectedMetric.value) {
        case CustomSensorMetric.Temp:
            return t('views.customSensors.fileUnit.temp')
        case CustomSensorMetric.Duty:
            return t('views.customSensors.fileUnit.duty')
        case CustomSensorMetric.RPM:
            return t('views.customSensors.fileUnit.rpm')
        case CustomSensorMetric.Freq:
            return t('views.customSensors.fileUnit.freq')
        case CustomSensorMetric.Watts:
            return t('views.customSensors.fileUnit.watts')
        default:
            return ''
    }
})
const offsetMax = computed(() => offsetLimit(selectedMetric.value))

const sourceOptionGroups = computed(() =>
    sourceGroups.value.map((group) => ({
        label: group.deviceName,
        options: group.sources.map((source) => ({
            label: source.frontendName,
            value: sourceKey(source),
            color: source.lineColor,
            rightText: `${source.value} ${sourceUnit(source)}`,
        })),
    })),
)
const chosenMixSourceKeys = computed<string[] | string | undefined>({
    get: () => chosenMixSources.value.map(sourceKey),
    set: (keys) => {
        if (!Array.isArray(keys)) return
        chosenMixSources.value = keys
            .map((key) => findSource(key))
            .filter((source): source is AvailableSource => source != null)
    },
})
const chosenSingleSourceKey = computed<string | string[] | undefined>({
    get: () => (chosenSingleSource.value != null ? sourceKey(chosenSingleSource.value) : undefined),
    set: (key) => {
        const source = findSource(key)
        if (source != null) chosenSingleSource.value = source
    },
})

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
// The sensor's page under Monitoring, linked from the header and from the chart.
const fullChartRoute: RouteLocationRaw = {
    name: 'monitoring-sensor',
    params: { deviceUID: customSensorsDeviceUID, channelName: customSensor.id },
}

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

const saveButtonDisabled = (): boolean =>
    !canSave({
        type: selectedSensorType.value,
        metric: selectedMetric.value,
        mixSourceCount: chosenMixSources.value.length,
        hasSingleSource: chosenSingleSource.value != null,
        filePath: filePath.value,
        scale: selectedScale.value,
        offset: selectedOffset.value,
        timeWindowSeconds: selectedTimeWindowSeconds.value,
    })

onMounted(async () => {
    watch(rawStore.currentDeviceStatus, () => {
        updateValues()
    })
    watch(settingsStore.allUIDeviceSettings, async () => {
        refreshSources()
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true })()
    })
    watch(
        [
            selectedMetric,
            selectedSensorType,
            selectedMixFunction,
            filePath,
            chosenMixSources,
            selectedScale,
            selectedOffset,
            selectedTimeWindowSeconds,
            chosenSingleSource,
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
                <!-- The margin sets it apart from delete and save, which belong together. -->
                <UiButton
                    v-if="!shouldCreateSensor"
                    class="mr-6"
                    variant="outline"
                    v-tooltip.top="t('layout.shell.coolingPage.fullChart')"
                    @click="router.push(fullChartRoute)"
                >
                    <svg-icon type="mdi" :path="mdiChartLine" :size="deviceStore.getREMSize(1.1)" />
                    <span class="ml-1">{{ t('layout.shell.monitoring') }}</span>
                </UiButton>
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
                            {{ t('views.customSensors.metric') }}
                        </small>
                        <!-- The metric decides whether the sensor is a temp or a channel of
                             its device, so it is fixed once the sensor exists. -->
                        <div
                            v-tooltip.top="{
                                escape: false,
                                value: t('views.customSensors.metricTooltip'),
                            }"
                        >
                            <UiSelect
                                :model-value="selectedMetric"
                                :options="metricOptions"
                                :disabled="!shouldCreateSensor"
                                class="w-full"
                                @update:model-value="changeMetric"
                            />
                        </div>
                        <p
                            v-if="selectedMetric === CustomSensorMetric.RPM"
                            class="ml-3 mt-1 text-sm text-text-color-secondary"
                        >
                            {{ t('views.customSensors.rpmUnitHint') }}
                        </p>
                        <small class="ml-3 mt-4 block font-light text-sm text-text-color-secondary">
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
                            {{ t('views.customSensors.scale') }}
                        </small>
                        <div
                            class="rounded-lg border border-border-one bg-bg-two p-3 flex justify-center"
                            v-tooltip.top="{
                                escape: false,
                                value: t('views.customSensors.scaleTooltip'),
                            }"
                        >
                            <UiNumberInput
                                v-model="selectedScale"
                                :min="-SCALE_MAGNITUDE_MAX"
                                :max="SCALE_MAGNITUDE_MAX"
                                :step="0.1"
                            />
                        </div>
                        <small class="ml-3 mt-4 mb-1 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.offset') }}
                        </small>
                        <div
                            class="rounded-lg border border-border-one bg-bg-two p-3 flex justify-center"
                            v-tooltip.top="{
                                escape: false,
                                value: t('views.customSensors.offsetTooltip'),
                            }"
                        >
                            <UiNumberInput
                                v-model="selectedOffset"
                                :min="-offsetMax"
                                :max="offsetMax"
                            />
                        </div>
                    </div>
                    <div
                        v-if="isSmoothingType(selectedSensorType)"
                        class="flex flex-col mt-1 w-full max-w-96 mb-28"
                    >
                        <small class="ml-3 mb-1 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.timeWindow') }}
                        </small>
                        <div
                            class="rounded-lg border border-border-one bg-bg-two p-3 flex justify-center"
                            v-tooltip.top="{
                                escape: false,
                                value: t('views.customSensors.timeWindowTooltip'),
                            }"
                        >
                            <UiNumberInput
                                v-model="selectedTimeWindowSeconds"
                                :min="TIME_WINDOW_SECONDS_MIN"
                                :max="TIME_WINDOW_SECONDS_MAX"
                                :suffix="t('common.secondAbbr')"
                            />
                        </div>
                    </div>
                    <div
                        v-else-if="selectedSensorType === CustomSensorType.File"
                        class="flex flex-col w-full max-w-96 mt-1"
                    >
                        <small class="ml-3 mb-1 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.sensorFile') }}
                        </small>
                        <UiInput
                            v-model="filePath"
                            class="w-full"
                            placeholder="/tmp/your_sensor_file"
                            :class="{ '!border-error': !filePath }"
                            v-tooltip.top="t('views.customSensors.filePathTooltip')"
                        />
                        <p class="ml-3 mt-1 text-sm text-text-color-secondary">
                            {{ fileUnitText }}
                        </p>
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
                    class="flex flex-col lg:flex-row mt-4 w-full"
                >
                    <div class="w-full max-w-xl lg:mr-4">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.sources') }}
                        </small>
                        <UiGroupedListbox
                            v-model="chosenMixSourceKeys"
                            class="w-full max-h-[28rem]"
                            :groups="sourceOptionGroups"
                            filter
                            :filter-placeholder="t('common.search')"
                            multiple
                            :invalid="chosenMixSources.length === 0"
                            v-tooltip.top="{
                                escape: false,
                                value: t('views.customSensors.sourcesTooltip'),
                            }"
                        />
                    </div>
                    <div
                        v-if="selectedMixFunction === CustomSensorMixFunctionType.WeightedAvg"
                        class="w-full max-w-xl mt-4 lg:mt-0"
                        v-tooltip.top="t('views.customSensors.sourceWeights')"
                    >
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.sourceWeights') }}
                        </small>
                        <UiTable bordered>
                            <template #head>
                                <tr>
                                    <th class="w-full">
                                        {{ t('views.customSensors.sourceName') }}
                                    </th>
                                    <th>{{ t('views.customSensors.weight') }}</th>
                                </tr>
                            </template>
                            <tr v-for="source in chosenMixSources" :key="sourceKey(source)">
                                <td>
                                    <div class="flex items-center gap-2">
                                        <svg-icon
                                            type="mdi"
                                            :path="mdiMinusThick"
                                            :size="14"
                                            class="shrink-0"
                                            :style="{ color: source.lineColor }"
                                        />
                                        {{ source.frontendName }}
                                    </div>
                                </td>
                                <td>
                                    <UiNumberInput v-model="source.weight" :min="1" :max="254" />
                                </td>
                            </tr>
                        </UiTable>
                    </div>
                </div>
                <!--The single-source types share one selection-->
                <div
                    v-if="isSingleSourceType(selectedSensorType)"
                    class="flex flex-col lg:flex-row mt-0 w-full"
                >
                    <div class="w-full max-w-xl lg:mr-4">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.customSensors.source') }}
                        </small>
                        <UiGroupedListbox
                            v-model="chosenSingleSourceKey"
                            class="w-full mt-1 max-h-[28rem]"
                            :groups="sourceOptionGroups"
                            filter
                            :filter-placeholder="t('common.search')"
                            :invalid="chosenSingleSource == null"
                        />
                    </div>
                </div>
                <div
                    v-if="!shouldCreateSensor"
                    class="mt-6 shrink-0"
                    style="--time-chart-height: 24rem"
                    @wheel.capture="onChartWheelCapture"
                >
                    <TimeChart
                        :key="chartKey"
                        :dashboard="singleDashboard"
                        @line-set-changed="chartKey = uuidV4()"
                    />
                    <div class="mt-1 flex justify-center">
                        <RouterLink
                            :to="fullChartRoute"
                            class="flex items-center gap-1 rounded-lg px-2 py-1 text-sm text-text-color-secondary outline-none hover:bg-surface-hover hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                        >
                            <svg-icon type="mdi" :path="mdiChartLine" :size="16" />
                            {{ t('layout.shell.coolingPage.fullChart') }}
                        </RouterLink>
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
    </div>
</template>

<style scoped lang="scss"></style>
