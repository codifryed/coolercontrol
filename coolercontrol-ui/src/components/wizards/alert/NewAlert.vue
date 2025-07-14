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
import { mdiMemory } from '@mdi/js'
import Button from 'primevue/button'
import InputNumber from 'primevue/inputnumber'
import { computed, onMounted, ref, type Ref, watch } from 'vue'
import { DEFAULT_NAME_STRING_LENGTH, useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { storeToRefs } from 'pinia'
import { Alert } from '@/models/Alert.ts'
import { ChannelMetric, ChannelSource } from '@/models/ChannelSource.ts'
import Slider from 'primevue/slider'
import { useI18n } from 'vue-i18n'
import InputText from 'primevue/inputtext'
import Select from 'primevue/select'

interface Props {
    alert?: Alert
}

const defaultMin: number = 0.0

interface AvailableChannel {
    deviceUID: string // needed here as well for the dropdown selector
    channelName: string
    channelFrontendName: string
    lineColor: string
    value: string
    metric: ChannelMetric
}

interface AvailableChannelSources {
    deviceUID: string
    deviceName: string
    channels: Array<AvailableChannel>
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'newAlert', alert: Alert): void
    (e: 'name', name: string): void
    (e: 'close'): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const pollRate: Ref<number> = ref(settingsStore.ccSettings.poll_rate)

const collectAlert = (): Alert => {
    const newAlertName = `${t('views.alerts.newAlert')} ${settingsStore.alerts.length + 1}`
    const channelSource = new ChannelSource('', '', ChannelMetric.Temp)
    return new Alert(newAlertName, channelSource, defaultMin, 100, pollRate.value)
}
const alert: Alert = props.alert === undefined ? collectAlert() : props.alert
const chosenChannelSource: Ref<AvailableChannel | undefined> = ref()
const chosenMin: Ref<number> = ref(alert.min)
const chosenMax: Ref<number> = ref(alert.max)
const chosenName: Ref<string> = ref(alert.name)
const chosenWarmupDuration: Ref<number> = ref(alert.warmup_duration)
const warmupDurationStep: Ref<number> = pollRate
const maxWarmupDuration: Ref<number> = ref(pollRate.value * 5)

const channelSources: Ref<Array<AvailableChannelSources>> = ref([])
const fillChannelSources = (): void => {
    channelSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.status.temps.length === 0 && device.status.channels.length === 0) {
            continue
        }
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        const deviceSource: AvailableChannelSources = {
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            channels: [],
        }
        const addAvailableChannel = (
            channelName: string,
            value: number,
            metric: ChannelMetric,
        ): void => {
            deviceSource.channels.push({
                deviceUID: device.uid,
                channelName: channelName,
                channelFrontendName: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                lineColor: deviceSettings.sensorsAndChannels.get(channelName)!.color,
                value: value.toFixed(0),
                metric: metric,
            })
        }
        for (const temp of device.status.temps) {
            deviceSource.channels.push({
                deviceUID: device.uid,
                channelName: temp.name,
                channelFrontendName: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                lineColor: deviceSettings.sensorsAndChannels.get(temp.name)!.color,
                value: temp.temp.toFixed(1),
                metric: ChannelMetric.Temp,
            })
        }
        for (const channel of device.status.channels) {
            // Duties and RPMs are separate sources for Alerts:
            if (channel.duty != null) {
                const value = channel.duty
                const metric = channel.name.toLowerCase().includes('load')
                    ? ChannelMetric.Load
                    : ChannelMetric.Duty
                addAvailableChannel(channel.name, value, metric)
            }
            if (channel.rpm != null) {
                const value = channel.rpm
                const metric = ChannelMetric.RPM
                addAvailableChannel(channel.name, value, metric)
            }
        }
        if (deviceSource.channels.length === 0) {
            continue // when all of a devices channels are hidden
        }
        channelSources.value.push(deviceSource)
    }
}
fillChannelSources()
const nextStep = async (): Promise<void> => {
    alert.max = chosenMax.value
    alert.min = chosenMin.value
    alert.name = chosenName.value
    alert.warmup_duration = chosenWarmupDuration.value
    alert.channel_source.device_uid = chosenChannelSource.value?.deviceUID!
    alert.channel_source.channel_name = chosenChannelSource.value?.channelName!
    alert.channel_source.channel_metric = chosenChannelSource.value?.metric!
    emit('newAlert', alert)
    emit('nextStep', 2)
}

const updateValues = (): void => {
    for (const channelDevice of channelSources.value) {
        for (const channel of channelDevice.channels) {
            switch (channel.metric) {
                case ChannelMetric.Duty:
                case ChannelMetric.Load:
                    channel.value =
                        currentDeviceStatus.value.get(channel.deviceUID)!.get(channel.channelName)!
                            .duty || '0'
                    break
                case ChannelMetric.RPM:
                    channel.value =
                        currentDeviceStatus.value.get(channel.deviceUID)!.get(channel.channelName)!
                            .rpm || '0'
                    break
                case ChannelMetric.Freq:
                    channel.value =
                        currentDeviceStatus.value.get(channel.deviceUID)!.get(channel.channelName)!
                            .freq || '0'
                    break
                case ChannelMetric.Temp:
                default:
                    channel.value =
                        currentDeviceStatus.value.get(channel.deviceUID)!.get(channel.channelName)!
                            .temp || '0.0'
                    break
            }
        }
    }
}

const stepSize = (metric: ChannelMetric | undefined): number => {
    switch (metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return 1
        case ChannelMetric.RPM:
        case ChannelMetric.Freq:
            return 100
        case ChannelMetric.Temp:
        default:
            return 0.1
    }
}

const valueMax = (metric: ChannelMetric | undefined): number => {
    switch (metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return 100
        case ChannelMetric.RPM:
        case ChannelMetric.Freq:
            return 10_000
        case ChannelMetric.Temp:
        default:
            return 200
    }
}

const changeChannelSource = (event: any): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    chosenChannelSource.value = event.value
    chosenMax.value = valueMax(chosenChannelSource.value?.metric)
}

const valueSuffix = (metric: ChannelMetric | undefined): string => {
    switch (metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return ' %'
        case ChannelMetric.RPM:
            return ` ${t('common.rpmAbbr')}`
        case ChannelMetric.Freq:
            return ` ${t('common.mhzAbbr')}`
        case ChannelMetric.Temp:
        default:
            return ' °'
    }
}

const minScrolled = (event: WheelEvent): void => {
    if (chosenMin.value == null) return
    const step = stepSize(chosenChannelSource.value?.metric)
    if (event.deltaY < 0) {
        const max =
            chosenMax.value - (chosenChannelSource.value?.metric !== ChannelMetric.RPM ? 1 : 100)
        if (chosenMin.value < max) chosenMin.value += step
    } else {
        if (chosenMin.value >= step) chosenMin.value -= step
    }
}
const maxScrolled = (event: WheelEvent): void => {
    if (chosenMax.value == null) return
    const step = stepSize(chosenChannelSource.value?.metric)
    if (event.deltaY < 0) {
        const max = valueMax(chosenChannelSource.value?.metric)
        if (chosenMax.value < max) chosenMax.value += step
    } else {
        const min =
            chosenMin.value + (chosenChannelSource.value?.metric !== ChannelMetric.RPM ? 1 : 100)
        if (chosenMax.value >= min) chosenMax.value -= step
    }
}
const addScrollEventListeners = (): void => {
    // @ts-ignore
    document?.querySelector('#min-input')?.addEventListener('wheel', minScrolled)
    // @ts-ignore
    document?.querySelector('#max-input')?.addEventListener('wheel', maxScrolled)
}

const nameInvalid = computed(() => {
    return chosenName.value.length < 1 || chosenName.value.length > DEFAULT_NAME_STRING_LENGTH
})

onMounted(async () => {
    watch(currentDeviceStatus, () => {
        updateValues()
    })
    watch(settingsStore.allUIDeviceSettings, async () => {
        fillChannelSources()
    })
    addScrollEventListeners()
})
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div class="mt-0">
                <small class="ml-3 font-light text-sm"> {{ t('common.name') }}: </small>
                <div class="mt-0 flex flex-col">
                    <InputText
                        v-model="chosenName"
                        :placeholder="t('common.name')"
                        ref="inputArea"
                        id="property-name"
                        class="w-full h-11"
                        :invalid="nameInvalid"
                        :input-style="{ background: 'rgb(var(--colors-bg-one))' }"
                    />
                </div>
            </div>
            <div class="">
                <small class="ml-3 font-light text-sm">
                    {{ t('views.alerts.channelSource') }}
                </small>
                <Select
                    :model-value="chosenChannelSource"
                    class="w-full mr-3 h-11 bg-bg-one !justify-end"
                    :options="channelSources"
                    filter
                    checkmark
                    option-label="channelFrontendName"
                    option-group-label="deviceName"
                    option-group-children="channels"
                    :filter-placeholder="t('common.search')"
                    :invalid="chosenChannelSource == null"
                    v-tooltip.right="t('views.alerts.channelSourceTooltip')"
                    :placeholder="t('views.alerts.channelSource')"
                    @change="changeChannelSource"
                    scroll-height="40rem"
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
                                />{{ slotProps.option.channelFrontendName }}
                            </div>
                            <div>
                                {{ slotProps.option.value + valueSuffix(slotProps.option.metric) }}
                            </div>
                        </div>
                    </template>
                </Select>
            </div>
            <div class="">
                <small class="ml-3 font-light text-sm">
                    {{ t('views.alerts.triggerConditions') }}
                </small>
                <div class="pr-1 w-full border-border-one border-2 rounded-lg">
                    <table class="bg-bg-two rounded-lg w-full">
                        <tbody>
                            <tr v-tooltip.right="t('views.alerts.maxValueTooltip')">
                                <td
                                    class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2"
                                >
                                    <div class="text-right float-right">
                                        {{ t('views.alerts.greaterThan') }}
                                    </div>
                                </td>
                                <td class="py-4 px-4 w-60 leading-none items-center text-center">
                                    <InputNumber
                                        id="max-input"
                                        class="w-full"
                                        v-model="chosenMax"
                                        show-buttons
                                        :min="
                                            chosenMin +
                                            (chosenChannelSource?.metric !== ChannelMetric.RPM
                                                ? 1
                                                : 100)
                                        "
                                        :max="valueMax(chosenChannelSource?.metric)"
                                        :step="stepSize(chosenChannelSource?.metric)"
                                        :min-fraction-digits="
                                            chosenChannelSource?.metric !== ChannelMetric.Temp
                                                ? 0
                                                : 1
                                        "
                                        :suffix="valueSuffix(chosenChannelSource?.metric)"
                                        button-layout="horizontal"
                                        :input-style="{ width: '8rem' }"
                                        :disabled="chosenChannelSource == null"
                                    >
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                    <div class="mx-1.5 mt-3">
                                        <Slider
                                            v-model="chosenMax"
                                            class="!w-full"
                                            :step="stepSize(chosenChannelSource?.metric)"
                                            :min="
                                                chosenMin +
                                                (chosenChannelSource?.metric !== ChannelMetric.RPM
                                                    ? 1
                                                    : 100)
                                            "
                                            :max="valueMax(chosenChannelSource?.metric)"
                                            :disabled="chosenChannelSource == null"
                                        />
                                    </div>
                                </td>
                            </tr>
                            <tr v-tooltip.right="t('views.alerts.minValueTooltip')">
                                <td
                                    class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-t-2"
                                >
                                    <div class="text-right float-right">
                                        {{ t('views.alerts.lessThan') }}
                                    </div>
                                </td>
                                <td
                                    class="py-4 px-4 w-60 leading-none items-center text-center border-border-one border-t-2"
                                >
                                    <InputNumber
                                        id="min-input"
                                        class="w-full"
                                        v-model="chosenMin"
                                        show-buttons
                                        :min="0"
                                        :max="
                                            chosenMax -
                                            (chosenChannelSource?.metric !== ChannelMetric.RPM
                                                ? 1
                                                : 100)
                                        "
                                        :step="stepSize(chosenChannelSource?.metric)"
                                        :min-fraction-digits="
                                            chosenChannelSource?.metric !== ChannelMetric.Temp
                                                ? 0
                                                : 1
                                        "
                                        :suffix="valueSuffix(chosenChannelSource?.metric)"
                                        button-layout="horizontal"
                                        :input-style="{ width: '8rem' }"
                                        :disabled="chosenChannelSource == null"
                                    >
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                    <div class="mx-1.5 mt-3">
                                        <Slider
                                            v-model="chosenMin"
                                            class="!w-full"
                                            :step="stepSize(chosenChannelSource?.metric)"
                                            :min="0"
                                            :max="
                                                chosenMax -
                                                (chosenChannelSource?.metric !== ChannelMetric.RPM
                                                    ? 1
                                                    : 100)
                                            "
                                            :disabled="chosenChannelSource == null"
                                        />
                                    </div>
                                </td>
                            </tr>
                            <tr v-tooltip.right="t('views.alerts.warmupDurationTooltip')">
                                <td
                                    class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-t-2"
                                >
                                    <div class="text-right float-right">
                                        {{ t('views.alerts.warmupGreaterThan') }}
                                    </div>
                                </td>
                                <td
                                    class="py-4 px-4 w-60 leading-none items-center text-center border-border-one border-t-2"
                                >
                                    <InputNumber
                                        id="warmup-duration"
                                        class="w-full"
                                        v-model="chosenWarmupDuration"
                                        show-buttons
                                        :min="0"
                                        :max="maxWarmupDuration"
                                        :step="warmupDurationStep"
                                        :min-fraction-digits="1"
                                        :suffix="'s'"
                                        button-layout="horizontal"
                                        :input-style="{ width: '8rem' }"
                                        :disabled="chosenChannelSource == null"
                                        :invalid="chosenWarmupDuration % warmupDurationStep != 0"
                                    >
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                    <div class="mx-1.5 mt-3">
                                        <Slider
                                            v-model="chosenWarmupDuration"
                                            class="!w-full"
                                            :step="warmupDurationStep"
                                            :min="0"
                                            :max="maxWarmupDuration"
                                            :disabled="chosenChannelSource == null"
                                            :invalid="
                                                chosenWarmupDuration % warmupDurationStep != 0
                                            "
                                        />
                                    </div>
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    </div>
    <div class="flex flex-row justify-between mt-4">
        <Button class="w-24 bg-bg-one" :label="t('common.cancel')" @click="emit('close')" />
        <Button
            class="w-24 bg-bg-one"
            :label="t('common.next')"
            :disabled="chosenChannelSource == null || chosenName.length === 0"
            @click="nextStep"
        />
    </div>
</template>

<style scoped lang="scss"></style>
