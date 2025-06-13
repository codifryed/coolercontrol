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
import { mdiContentSaveOutline, mdiMemory } from '@mdi/js'
import Button from 'primevue/button'
import InputNumber from 'primevue/inputnumber'
import { inject, onMounted, ref, type Ref, watch } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { storeToRefs } from 'pinia'
import Listbox, { ListboxChangeEvent } from 'primevue/listbox'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { onBeforeRouteLeave, onBeforeRouteUpdate, useRouter } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'
import { Alert } from '@/models/Alert.ts'
import { ChannelMetric, ChannelSource } from '@/models/ChannelSource.ts'
import Slider from 'primevue/slider'
import { Emitter, EventType } from 'mitt'
import { useI18n } from 'vue-i18n'

interface Props {
    alertUID?: string
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
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const router = useRouter()
const { t } = useI18n()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const confirm = useConfirm()

const contextIsDirty: Ref<boolean> = ref(false)
const shouldCreateAlert: boolean = !props.alertUID

const collectAlert = async (): Promise<Alert> => {
    if (shouldCreateAlert) {
        const newAlertName = `${t('views.alerts.newAlert')} ${settingsStore.alerts.length + 1}`
        const channelSource = new ChannelSource('', '', ChannelMetric.Temp)
        return new Alert(newAlertName, channelSource, defaultMin, 100)
    } else {
        const foundAlert = settingsStore.alerts.find((alert) => alert.uid === props.alertUID!)
        if (foundAlert == undefined) {
            throw new Error(`Illegal State: Could not find Alert with UID: ${props.alertUID}`)
        }
        return foundAlert
    }
}
const alert: Alert = await collectAlert()
const chosenChannelSource: Ref<AvailableChannel | undefined> = ref()
const chosenMin: Ref<number> = ref(alert.min)
const chosenMax: Ref<number> = ref(alert.max)
const chosenName: Ref<string> = ref(alert.name)

const channelSources: Ref<Array<AvailableChannelSources>> = ref([])
const fillChannelSources = async (): Promise<void> => {
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
await fillChannelSources()
const startingChannelSource = () => {
    if (shouldCreateAlert) return undefined
    const deviceUID = alert.channel_source.device_uid
    const channelName = alert.channel_source.channel_name
    const channelMetric = alert.channel_source.channel_metric
    for (const device of channelSources.value) {
        if (device.deviceUID !== deviceUID) {
            continue
        }
        for (const channel of device.channels) {
            if (channel.channelName === channelName && channel.metric === channelMetric) {
                return channel
            }
        }
    }
    return undefined
}
chosenChannelSource.value = startingChannelSource()

const saveAlert = async (): Promise<void> => {
    alert.max = chosenMax.value
    alert.min = chosenMin.value
    alert.name = chosenName.value
    alert.channel_source.device_uid = chosenChannelSource.value?.deviceUID!
    alert.channel_source.channel_name = chosenChannelSource.value?.channelName!
    alert.channel_source.channel_metric = chosenChannelSource.value?.metric!
    if (shouldCreateAlert) {
        const successful = await settingsStore.createAlert(alert)
        if (successful) {
            await settingsStore.loadAlertsAndLogs()
            emitter.emit('alert-add-menu', { alertUID: alert.uid })
            contextIsDirty.value = false
            await router.push({ name: 'alerts', params: { alertUID: alert.uid } })
        }
    } else {
        const successful = await settingsStore.updateAlert(alert.uid)
        if (successful) contextIsDirty.value = false
    }
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

const changeChannelSource = (event: ListboxChangeEvent): void => {
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

const checkForUnsavedChanges = (_to: any, _from: any, next: any): void => {
    if (!contextIsDirty.value) {
        next()
        return
    }
    confirm.require({
        message: t('views.alerts.unsavedChanges'),
        header: t('views.alerts.unsavedChangesHeader'),
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        rejectLabel: t('common.stay'),
        acceptLabel: t('common.discard'),
        accept: () => {
            next()
            contextIsDirty.value = false
        },
        reject: () => next(false),
    })
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

onMounted(async () => {
    watch(currentDeviceStatus, () => {
        updateValues()
    })
    watch(settingsStore.allUIDeviceSettings, async () => {
        await fillChannelSources()
    })
    watch([chosenChannelSource, chosenMax, chosenMin, chosenName], () => {
        contextIsDirty.value = true
    })
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)
    addScrollEventListeners()
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="flex pl-4 py-2 text-2xl overflow-hidden">
            <span class="font-bold overflow-hidden overflow-ellipsis">{{ alert.name }}</span>
        </div>
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div class="p-2">
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    :class="{ 'animate-pulse-fast': contextIsDirty }"
                    label="Save"
                    v-tooltip.bottom="t('views.alerts.saveAlert')"
                    :disabled="chosenChannelSource == null || chosenName.length === 0"
                    @click="saveAlert"
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
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="p-4 pb-16 h-screen w-full">
            <div class="flex flex-col-reverse lg:flex-row mt-0 w-full">
                <div class="w-96 mr-4">
                    <small class="ml-3 font-light text-sm text-text-color-secondary">
                        {{ t('views.alerts.channelSource') }}
                    </small>
                    <Listbox
                        :model-value="chosenChannelSource"
                        class="w-full mt-1 mb-6"
                        :options="channelSources"
                        filter
                        checkmark
                        option-label="channelFrontendName"
                        option-group-label="deviceName"
                        option-group-children="channels"
                        :filter-placeholder="t('common.search')"
                        list-style="max-height: 100%"
                        :invalid="chosenChannelSource == null"
                        v-tooltip.right="t('views.alerts.channelSourceTooltip')"
                        @change="changeChannelSource"
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
                                    {{
                                        slotProps.option.value +
                                        valueSuffix(slotProps.option.metric)
                                    }}
                                </div>
                            </div>
                        </template>
                    </Listbox>
                </div>
                <div class="mt-1 w-96">
                    <!--                    <small class="ml-3 font-light text-sm text-text-color-secondary">-->
                    <!--                        Name-->
                    <!--                    </small>-->
                    <!--                    <div class="mt-1 mb-3">-->
                    <!--                        <InputText-->
                    <!--                            ref="inputArea"-->
                    <!--                            id="name"-->
                    <!--                            v-model="chosenName"-->
                    <!--                            class="w-96"-->
                    <!--                            @keydown.enter="saveAlert"-->
                    <!--                            :placeholder="chosenName"-->
                    <!--                            v-tooltip.right="'Alert Name'"-->
                    <!--                            :invalid="chosenName.length === 0"-->
                    <!--                        />-->
                    <!--                    </div>-->
                    <small class="ml-3 font-light text-sm text-text-color-secondary">
                        {{ t('views.alerts.triggerConditions') }}
                    </small>
                    <table class="bg-bg-two rounded-lg mb-4">
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
                                    <Slider
                                        v-model="chosenMax"
                                        class="!w-48 ml-1"
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
                                    <Slider
                                        v-model="chosenMin"
                                        class="!w-48 ml-1"
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
                                </td>
                            </tr>
                        </tbody>
                    </table>
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
</template>

<style scoped lang="scss"></style>
