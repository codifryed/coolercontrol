<!--
  SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiArrowLeft } from '@mdi/js'
import UiButton from '@/shell/ui/UiButton.vue'
import UiGroupedSelect from '@/shell/ui/UiGroupedSelect.vue'
import { type UiOptionGroup } from '@/shell/ui/UiGroupedListbox.vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { computed, ref, Ref, toRaw, watch } from 'vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { ProfileTempSource } from '@/models/Profile.ts'
import { useProfileLimitInfo, type LimitInfo } from '@/composables/useProfileLimitInfo.ts'

interface Props {
    name: string
    tempSource: ProfileTempSource | undefined
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'tempSource', tempSource: ProfileTempSource): void
}>()

const { t } = useI18n()
const { getLimitInfo } = useProfileLimitInfo()
const deviceStore = useDeviceStore()
// We need to use the raw state to watch for changes, as the pinia reactive proxy isn't properly
// reacting to changes from Vue's shallowRef & triggerRef anymore.
const rawStore = toRaw(deviceStore.$state)
const settingsStore = useSettingsStore()

interface AvailableTemp {
    deviceUID: string // needed here as well for the dropdown selector
    tempName: string
    tempFrontendName: string
    lineColor: string
    temp: string
    limitInfo: LimitInfo | null
}

interface AvailableTempSources {
    deviceUID: string
    deviceName: string
    profileMinLength: number
    profileMaxLength: number
    amdGpuOverdrive?: boolean
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
            amdGpuOverdrive: device.info.amd_gpu_overdrive,
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
                limitInfo: getLimitInfo({
                    profileMaxLength: deviceSource.profileMaxLength,
                    amdGpuOverdrive: deviceSource.amdGpuOverdrive,
                    tempName: temp.name,
                }),
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
const tempKey = (deviceUID: string, tempName: string): string => `${deviceUID}/${tempName}`
const tempSourceGroups = computed<UiOptionGroup[]>(() =>
    tempSources.value.map((device) => ({
        label: device.deviceName,
        color: settingsStore.allUIDeviceSettings.get(device.deviceUID)?.userColor,
        options: device.temps.map((temp) => ({
            label: temp.tempFrontendName,
            value: tempKey(temp.deviceUID, temp.tempName),
            color: temp.lineColor,
            rightText:
                temp.limitInfo != null
                    ? `${temp.limitInfo.badge} · ${temp.temp} ${t('common.tempUnit')}`
                    : `${temp.temp} ${t('common.tempUnit')}`,
        })),
    })),
)
const chosenTempKey = computed<string | undefined>({
    get: () =>
        chosenTemp.value != null
            ? tempKey(chosenTemp.value.deviceUID, chosenTemp.value.tempName)
            : undefined,
    set: (key) => {
        chosenTemp.value = tempSources.value
            .flatMap((device) => device.temps)
            .find((temp) => tempKey(temp.deviceUID, temp.tempName) === key)
    },
})
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
                deviceStore.currentDeviceStatus
                    .get(availableTemp.deviceUID)!
                    .get(availableTemp.tempName)!.temp || '0.0'
        }
    }
}

watch(settingsStore.allUIDeviceSettings, () => {
    // update all temp sources:
    fillTempSources()
})
watch(rawStore.currentDeviceStatus, () => {
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
                <UiGroupedSelect
                    v-model="chosenTempKey"
                    :groups="tempSourceGroups"
                    :placeholder="t('views.profiles.tempSource')"
                    filter
                    :filter-placeholder="t('common.search')"
                    :invalid="chosenTemp == null"
                    class="w-full"
                />
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <UiButton variant="ghost" class="w-24 bg-bg-one" @click="emit('nextStep', 3)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </UiButton>
            <UiButton
                variant="ghost"
                class="w-24 bg-bg-one"
                :disabled="chosenTemp == null"
                @click="nextStep"
            >
                {{ t('common.next') }}
            </UiButton>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
