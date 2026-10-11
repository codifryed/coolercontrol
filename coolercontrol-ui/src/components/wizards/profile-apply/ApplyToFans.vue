<!--
  SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { computed, onMounted, ref, Ref, toRaw, watch } from 'vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { mdiContentSaveOutline, mdiTagOutline } from '@mdi/js'
import UiButton from '@/shell/ui/UiButton.vue'
import UiMultiSelect from '@/shell/ui/UiMultiSelect.vue'
import { type UiOptionGroup } from '@/shell/ui/UiGroupedListbox.vue'
import { DeviceType, UID } from '@/models/Device.ts'
import { ChannelMetric } from '@/models/ChannelSource.ts'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings.ts'
import { useRoute, useRouter } from 'vue-router'

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
// We need to use the raw state to watch for changes, as the pinia reactive proxy isn't properly
// reacting to changes from Vue's shallowRef & triggerRef anymore.
const rawStore = toRaw(deviceStore.$state)
const settingsStore = useSettingsStore()
const router = useRouter()
const route = useRoute()
const { t } = useI18n()

const availableControlChannels: Ref<Array<AvailableChannelSources>> = ref([])
const chosenChannels: Ref<Array<AvailableChannel>> = ref([])
const selectionMode: Ref<'channel' | 'tag'> = ref('channel')
const selectedTags: Ref<Array<string>> = ref([])
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

const valueSuffix = (channel: AvailableChannel): string => {
    switch (channel.metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return ` ${t('common.percentUnit')}`
        case ChannelMetric.RPM:
            return ` ${settingsStore.rpmUnit(channel.deviceUID, channel.channelName)}`
        case ChannelMetric.Freq:
            return ` ${t('common.mhzAbbr')}`
        case ChannelMetric.Temp:
        default:
            return ` ${t('common.tempUnit')}`
    }
}

const channelKey = (deviceUID: string, channelName: string): string => `${deviceUID}/${channelName}`
const channelGroups = computed<UiOptionGroup[]>(() =>
    availableControlChannels.value.map((device) => ({
        label: device.deviceName,
        options: device.channels.map((ch) => ({
            label: ch.channelFrontendName,
            value: channelKey(ch.deviceUID, ch.channelName),
            color: ch.lineColor,
            rightText: `${ch.value}${valueSuffix(ch)}`,
        })),
    })),
)
const chosenChannelKeys = computed<string[]>({
    get: () => chosenChannels.value.map((ch) => channelKey(ch.deviceUID, ch.channelName)),
    set: (keys) => {
        chosenChannels.value = keys
            .map((key) =>
                availableControlChannels.value
                    .flatMap((d) => d.channels)
                    .find((ch) => channelKey(ch.deviceUID, ch.channelName) === key),
            )
            .filter((ch): ch is AvailableChannel => ch != null)
    },
})

const updateValues = (): void => {
    for (const channelDevice of availableControlChannels.value) {
        for (const channel of channelDevice.channels) {
            switch (channel.metric) {
                case ChannelMetric.RPM:
                    channel.value =
                        deviceStore.currentDeviceStatus
                            .get(channel.deviceUID)!
                            .get(channel.channelName)!.rpm || '0'
                    break
                case ChannelMetric.Duty:
                default:
                    channel.value =
                        deviceStore.currentDeviceStatus
                            .get(channel.deviceUID)!
                            .get(channel.channelName)!.duty || '0'
                    break
            }
        }
    }
}

const applyProfileToChannels = async (): Promise<void> => {
    const setting = new DeviceSettingWriteProfileDTO(props.profileUID)
    for (const channel of chosenChannels.value) {
        // we route away from device channels currently open to avoid UI conflicts
        if (
            route.params != null &&
            route.params.deviceUID === channel.deviceUID &&
            route.params.channelName === channel.channelName
        ) {
            await router.push({ name: 'startup-page' })
        }
        await settingsStore.saveDaemonDeviceSettingProfile(
            channel.deviceUID,
            channel.channelName,
            setting,
        )
    }
    emit('close')
}

interface TagOption {
    name: string
    color: string
    channelCount: number
}

const availableTags = computed((): Array<TagOption> => {
    const result: Array<TagOption> = []
    for (const [tagName, tagSettings] of settingsStore.tags) {
        const channels = settingsStore.getTagChannels(tagName)
        const controllableCount = channels.filter((ch) =>
            availableControlChannels.value.some(
                (src) =>
                    src.deviceUID === ch.deviceUID &&
                    src.channels.some((c) => c.channelName === ch.channelName),
            ),
        ).length
        if (controllableCount > 0) {
            result.push({
                name: tagName,
                color: tagSettings.color,
                channelCount: controllableCount,
            })
        }
    }
    return result
})

const applyTagSelection = (): void => {
    const newChannels: Array<AvailableChannel> = []
    for (const tagName of selectedTags.value) {
        const tagChannels = settingsStore.getTagChannels(tagName)
        for (const { deviceUID, channelName } of tagChannels) {
            const src = availableControlChannels.value.find((s) => s.deviceUID === deviceUID)
            if (src == null) continue
            const ch = src.channels.find((c) => c.channelName === channelName)
            if (ch == null) continue
            if (
                !newChannels.some((c) => c.deviceUID === deviceUID && c.channelName === channelName)
            ) {
                newChannels.push(ch)
            }
        }
    }
    chosenChannels.value = newChannels
}

onMounted(async () => {
    watch(rawStore.currentDeviceStatus, () => {
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
            <!-- Mode toggle -->
            <div class="flex rounded-lg overflow-hidden border border-border-one self-start">
                <button
                    class="px-3 py-1 text-sm"
                    :class="
                        selectionMode === 'channel'
                            ? 'bg-accent/80 text-text-color'
                            : 'bg-bg-one text-text-color-secondary hover:text-text-color'
                    "
                    @click="selectionMode = 'channel'"
                >
                    {{ t('components.wizards.profileApply.selectByChannel') }}
                </button>
                <button
                    class="px-3 py-1 text-sm flex items-center gap-x-1"
                    :class="
                        selectionMode === 'tag'
                            ? 'bg-accent/80 text-text-color'
                            : 'bg-bg-one text-text-color-secondary hover:text-text-color'
                    "
                    @click="selectionMode = 'tag'"
                >
                    <svg-icon type="mdi" :path="mdiTagOutline" :size="deviceStore.getREMSize(1)" />
                    {{ t('components.wizards.profileApply.selectByTag') }}
                </button>
            </div>

            <!-- Channel selection mode -->
            <template v-if="selectionMode === 'channel'">
                <small class="ml-3 font-light text-sm">
                    {{ t('components.wizards.profileApply.channelsApply') }}
                </small>
                <span
                    v-tooltip.top="{
                        escape: false,
                        value: t('components.wizards.profileApply.channelsTooltip'),
                    }"
                >
                    <UiMultiSelect
                        v-model="chosenChannelKeys"
                        :groups="channelGroups"
                        filter
                        :filter-placeholder="t('common.search')"
                        :invalid="chosenChannels.length === 0"
                        :placeholder="t('components.wizards.profileApply.selectChannels')"
                        class="w-full"
                    />
                </span>
            </template>

            <!-- Tag selection mode -->
            <template v-else>
                <small class="ml-3 font-light text-sm">
                    {{ t('components.wizards.profileApply.channelsApply') }}
                </small>
                <div
                    v-if="availableTags.length === 0"
                    class="ml-3 text-text-color-secondary text-sm"
                >
                    {{ t('components.wizards.profileApply.noTags') }}
                </div>
                <div v-else class="flex flex-col gap-y-1">
                    <div
                        v-for="tag in availableTags"
                        :key="tag.name"
                        class="flex items-center gap-x-2 px-3 py-1.5 rounded cursor-pointer hover:bg-bg-one"
                        :class="{ 'bg-bg-one': selectedTags.includes(tag.name) }"
                        @click="
                            () => {
                                selectedTags.includes(tag.name)
                                    ? selectedTags.splice(selectedTags.indexOf(tag.name), 1)
                                    : selectedTags.push(tag.name)
                                applyTagSelection()
                            }
                        "
                    >
                        <span
                            class="pi"
                            :class="selectedTags.includes(tag.name) ? 'pi-check-square' : 'pi-stop'"
                            :style="{ color: tag.color }"
                        />
                        <span
                            class="w-3 h-3 rounded-full"
                            :style="{ backgroundColor: tag.color }"
                        />
                        <span class="flex-1">{{ tag.name }}</span>
                        <span class="text-text-color-secondary text-xs"
                            >{{ tag.channelCount }}
                            {{
                                t('components.wizards.profileApply.tagFanCount', tag.channelCount)
                            }}</span
                        >
                    </div>
                </div>
                <!-- Summary of chosen channels -->
                <div v-if="chosenChannels.length > 0" class="mt-2 ml-3">
                    <small class="font-light text-sm text-text-color-secondary">
                        {{ chosenChannels.map((c) => c.channelFrontendName).join(', ') }}
                    </small>
                </div>
            </template>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <UiButton variant="ghost" class="w-24 bg-bg-one" @click="emit('close')">
                {{ t('common.cancel') }}
            </UiButton>
            <UiButton
                variant="solid"
                class="w-32"
                :disabled="chosenChannels.length === 0"
                v-tooltip.top="t('views.speed.applySetting')"
                @click="applyProfileToChannels"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiContentSaveOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </UiButton>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
