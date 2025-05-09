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
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useI18n } from 'vue-i18n'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings.ts'
import { v4 as uuidV4 } from 'uuid'

interface Props {
    deviceUID: UID
    channelName: string
    selectedProfileUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()
const { t } = useI18n()
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
const resetSettings = async (): Promise<void> => {
    const setting = new DeviceSettingWriteProfileDTO('0')
    await settingsStore.saveDaemonDeviceSettingProfile(props.deviceUID, props.channelName, setting)
    await router.push({
        name: 'device-speed',
        params: { deviceUID: props.deviceUID, channelName: props.channelName },
        query: { key: uuidV4() },
    })
    emit('close')
}
const channelLabel =
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)
        ?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName

// Non-controllable channels should never make it to this wizard
</script>

<template>
    <div class="flex flex-col gap-y-4 w-96">
        <p class="my-2 text-center text-lg">
            <span class="font-bold">{{ channelLabel }}</span> {{ t('views.dashboard.controls') }}
        </p>
        <div class="mt-0 flex flex-col place-items-center gap-y-6">
            <Button
                class="!p-2 h-[2.375rem]"
                :label="t('components.wizards.fanControl.manualSpeed')"
                @click="emit('nextStep', 4)"
            />
            <Button
                class="!p-2 h-[2.375rem]"
                :label="t('components.wizards.fanControl.createNewProfile')"
                @click="emit('nextStep', 3)"
            />
            <Button
                v-if="profilesLength > 1"
                class="!p-2 h-[2.375rem]"
                :label="t('components.wizards.fanControl.existingProfile')"
                @click="emit('nextStep', 2)"
            />
            <Button
                v-if="props.selectedProfileUID !== '0'"
                class="!p-2 h-[2.375rem]"
                :label="currentProfileMessage"
                @click="redirectProfileAndClose"
            />
            <Button
                v-if="selectedFunctionUID !== '0'"
                class="!p-2 h-[2.375rem]"
                :label="currentFunctionMessage"
                @click="redirectFunctionAndClose"
            />
            <Button
                v-if="props.selectedProfileUID !== '0'"
                class="!p-2 h-[2.375rem]"
                :label="t('components.wizards.fanControl.resetSettings')"
                @click="resetSettings"
            />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
