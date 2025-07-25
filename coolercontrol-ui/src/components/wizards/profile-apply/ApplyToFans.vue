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
import { onMounted, ref, Ref, watch } from 'vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { mdiContentSaveOutline, mdiMemory } from '@mdi/js'
import Button from 'primevue/button'
import { DeviceType, UID } from '@/models/Device.ts'
import MultiSelect from 'primevue/multiselect'
import { ChannelMetric } from '@/models/ChannelSource.ts'
import { storeToRefs } from 'pinia'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings.ts'

const emit = defineEmits<{
    (e: 'close'): void
}>()

const props = defineProps<{
    profileUID: UID
}>()

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

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const { t } = useI18n()

const availableControlChannels: Ref<Array<AvailableChannelSources>> = ref([])
const chosenChannels: Ref<Array<AvailableChannel>> = ref([])
const profileName =
    settingsStore.profiles.find((profile) => profile.uid === props.profileUID)?.name ?? 'unknown'

const fillAvailableChannelSources = (): void => {
    availableControlChannels.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.type === DeviceType.CUSTOM_SENSORS || device.type === DeviceType.CPU) {
            continue // no controls
        }
        if (device.info == null) continue
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        const availableChannelSource: AvailableChannelSources = {
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            channels: [],
        }

        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.speed_options == null) continue
            const isControllable: boolean = channelInfo.speed_options.fixed_enabled ?? false
            if (!isControllable) continue
            const addAvailableChannel = (
                channelName: string,
                value: number,
                metric: ChannelMetric,
            ): void => {
                availableChannelSource.channels.push({
                    deviceUID: device.uid,
                    channelName: channelName,
                    channelFrontendName: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                    lineColor: deviceSettings.sensorsAndChannels.get(channelName)!.color,
                    value: value.toFixed(0),
                    metric: metric,
                })
            }
            for (const channel of device.status.channels) {
                if (channel.name !== channelName) continue
                // Duties are preferred to display, with RPMs as backup
                if (channel.duty != null) {
                    const value = channel.duty
                    const metric = ChannelMetric.Duty
                    addAvailableChannel(channel.name, value, metric)
                } else if (channel.rpm != null) {
                    const value = channel.rpm
                    const metric = ChannelMetric.RPM
                    addAvailableChannel(channel.name, value, metric)
                }
            }
        }
        if (availableChannelSource.channels.length === 0) continue
        availableControlChannels.value.push(availableChannelSource)
    }
}
fillAvailableChannelSources()

const setAlreadyAppliedChannels = (): void => {
    for (const deviceSource of availableControlChannels.value) {
        const deviceDaemonSettings = settingsStore.allDaemonDeviceSettings.get(
            deviceSource.deviceUID,
        )
        for (const channelSource of deviceSource.channels) {
            if (
                deviceDaemonSettings?.settings.get(channelSource.channelName)?.profile_uid != null
            ) {
                if (
                    props.profileUID !=
                    deviceDaemonSettings?.settings.get(channelSource.channelName)!.profile_uid
                )
                    continue
                chosenChannels.value.push(channelSource)
            }
        }
    }
}
setAlreadyAppliedChannels()

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

const updateValues = (): void => {
    for (const channelDevice of availableControlChannels.value) {
        for (const channel of channelDevice.channels) {
            switch (channel.metric) {
                case ChannelMetric.RPM:
                    channel.value =
                        currentDeviceStatus.value.get(channel.deviceUID)!.get(channel.channelName)!
                            .rpm || '0'
                    break
                case ChannelMetric.Duty:
                default:
                    channel.value =
                        currentDeviceStatus.value.get(channel.deviceUID)!.get(channel.channelName)!
                            .duty || '0'
                    break
            }
        }
    }
}

const applyProfileToChannels = async (): Promise<void> => {
    const setting = new DeviceSettingWriteProfileDTO(props.profileUID)
    for (const channel of chosenChannels.value) {
        await settingsStore.saveDaemonDeviceSettingProfile(
            channel.deviceUID,
            channel.channelName,
            setting,
        )
    }
    emit('close')
}

onMounted(async () => {
    watch(currentDeviceStatus, () => {
        updateValues()
    })
    watch(settingsStore.allUIDeviceSettings, async () => {
        fillAvailableChannelSources()
    })
})
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <p class="my-2 text-center text-lg">
                <span class="font-bold">{{ profileName }}</span>
            </p>
            <small class="ml-3 font-light text-sm">
                {{ t('components.wizards.profileApply.channelsApply') }}
            </small>
            <MultiSelect
                v-model="chosenChannels"
                :options="availableControlChannels"
                class="w-full h-11 bg-bg-one items-center"
                filter
                checkmark
                option-label="channelFrontendName"
                option-group-label="deviceName"
                option-group-children="channels"
                :filter-placeholder="t('common.search')"
                :invalid="chosenChannels.length === 0"
                scroll-height="40rem"
                dropdown-icon="pi pi-gauge"
                :placeholder="t('components.wizards.profileApply.selectChannels')"
                v-tooltip.bottom="{
                    escape: false,
                    value: t('components.wizards.profileApply.channelsTooltip'),
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
                            />{{ slotProps.option.channelFrontendName }}
                        </div>
                        <div>
                            {{ slotProps.option.value + valueSuffix(slotProps.option.metric) }}
                        </div>
                    </div>
                </template>
            </MultiSelect>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" :label="t('common.cancel')" @click="emit('close')" />
            <Button
                class="bg-accent/80 hover:!bg-accent w-32"
                :label="t('common.apply')"
                :disabled="chosenChannels.length === 0"
                v-tooltip.bottom="t('views.speed.applySetting')"
                @click="applyProfileToChannels"
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
