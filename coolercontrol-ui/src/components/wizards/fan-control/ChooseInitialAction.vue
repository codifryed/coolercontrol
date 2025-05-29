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
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { UID } from '@/models/Device.ts'
import { useRouter } from 'vue-router'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useI18n } from 'vue-i18n'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings.ts'
import { v4 as uuidV4 } from 'uuid'
import {
    mdiChartBoxOutline,
    mdiChartBoxPlusOutline,
    mdiCogOutline,
    mdiFan,
    mdiPencilBoxOutline,
    mdiRestore,
} from '@mdi/js'

interface Props {
    deviceUID: UID
    channelName: string
    selectedProfileUID?: UID
    isControlView: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()
const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const router = useRouter()

const selectedProfileName: string =
    settingsStore.profiles.find((profile) => profile.uid === props.selectedProfileUID)?.name ??
    'Unknown'
const currentProfileMessage: string = `${t('components.wizards.fanControl.editCurrentProfile')}: "${selectedProfileName}"`
const profilesLength: number = settingsStore.profiles.length

const selectedFunctionUID: UID =
    settingsStore.profiles.find((profile) => profile.uid === props.selectedProfileUID)
        ?.function_uid ?? '0'
const selectedFunctionName: string =
    settingsStore.functions.find((fun) => fun.uid === selectedFunctionUID)?.name ?? 'Unknown'
const currentFunctionMessage: string = `${t('components.wizards.fanControl.editCurrentFunction')}: "${selectedFunctionName}"`

const redirectProfileAndClose = () => {
    router.push({ name: 'profiles', params: { profileUID: props.selectedProfileUID } })
    emit('close')
}
const redirectFunctionAndClose = () => {
    router.push({ name: 'functions', params: { functionUID: selectedFunctionUID } })
    emit('close')
}
const redirectSpeedViewAndClose = () => {
    router.push({
        name: 'device-speed',
        params: { deviceUID: props.deviceUID, channelName: props.channelName },
    })
    emit('close')
}
const resetSettings = async (): Promise<void> => {
    const setting = new DeviceSettingWriteProfileDTO('0')
    await settingsStore.saveDaemonDeviceSettingProfile(props.deviceUID, props.channelName, setting)
    if (!props.isControlView) {
        // Only redirect if we are not on the control view
        await router.push({
            name: 'device-speed',
            params: { deviceUID: props.deviceUID, channelName: props.channelName },
            query: { key: uuidV4() },
        })
    }
    emit('close')
}
const channelLabel =
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)
        ?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName

// Non-controllable channels should never make it to this wizard
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] h-[40vh] min-h-max">
        <div class="flex flex-col gap-y-4">
            <p class="my-2 text-center text-lg">
                <span class="font-bold">{{ channelLabel }}</span>
            </p>
            <div class="mt-0 flex flex-col place-items-center gap-y-3">
                <Button
                    v-if="isControlView"
                    class="!p-2 bg-bg-one w-full !justify-start"
                    @click="redirectSpeedViewAndClose"
                >
                    <div class="flex flex-row font-semibold items-center">
                        <svg-icon
                            class="outline-0 mr-2"
                            type="mdi"
                            :path="mdiFan"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('components.wizards.fanControl.currentSettings') }}
                    </div>
                </Button>
                <Button class="!p-2 bg-bg-one w-full !justify-start" @click="emit('nextStep', 4)">
                    <div class="flex flex-row font-semibold items-center">
                        <svg-icon
                            class="outline-0 mr-2"
                            type="mdi"
                            :path="mdiCogOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('components.wizards.fanControl.manualSpeed') }}
                    </div>
                </Button>
                <Button
                    v-if="profilesLength > 1"
                    class="!p-2 bg-bg-one w-full !justify-start"
                    :label="t('components.wizards.fanControl.existingProfile')"
                    @click="emit('nextStep', 2)"
                >
                    <div class="flex flex-row font-semibold items-center">
                        <svg-icon
                            class="outline-0 mr-2"
                            type="mdi"
                            :path="mdiChartBoxOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('components.wizards.fanControl.existingProfile') }}
                    </div>
                </Button>
                <Button class="!p-2 bg-bg-one w-full !justify-start" @click="emit('nextStep', 3)">
                    <div class="flex flex-row font-semibold items-center">
                        <svg-icon
                            class="outline-0 mr-2"
                            type="mdi"
                            :path="mdiChartBoxPlusOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('components.wizards.fanControl.createNewProfile') }}
                    </div>
                </Button>
                <Button
                    v-if="
                        props.selectedProfileUID !== undefined && props.selectedProfileUID !== '0'
                    "
                    class="!p-2 bg-bg-one w-full !justify-start"
                    @click="redirectProfileAndClose"
                >
                    <div class="flex flex-row font-semibold items-center">
                        <svg-icon
                            class="outline-0 mr-2"
                            type="mdi"
                            :path="mdiPencilBoxOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ currentProfileMessage }}
                    </div>
                </Button>
                <Button
                    v-if="selectedFunctionUID !== '0'"
                    class="!p-2 bg-bg-one w-full !justify-start"
                    @click="redirectFunctionAndClose"
                >
                    <div class="flex flex-row font-semibold items-center">
                        <svg-icon
                            class="outline-0 mr-2"
                            type="mdi"
                            :path="mdiPencilBoxOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ currentFunctionMessage }}
                    </div>
                </Button>
                <Button
                    v-if="props.selectedProfileUID == null || props.selectedProfileUID !== '0'"
                    class="!p-2 bg-bg-one w-full !justify-start"
                    @click="resetSettings"
                >
                    <div class="flex flex-row font-semibold items-center">
                        <svg-icon
                            class="outline-0 mr-2"
                            type="mdi"
                            :path="mdiRestore"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('components.wizards.fanControl.resetSettings') }}
                    </div>
                </Button>
            </div>
        </div>

        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" :label="t('common.cancel')" @click="emit('close')" />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
