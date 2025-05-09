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
import { mdiArrowLeft, mdiContentSaveOutline } from '@mdi/js'
import Button from 'primevue/button'
import { UID } from '@/models/Device.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings.ts'
import { Profile, ProfileType } from '@/models/Profile.ts'
import { useToast } from 'primevue/usetoast'
import { Emitter, EventType } from 'mitt'
import { inject } from 'vue'
import { v4 as uuidV4 } from 'uuid'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

interface Props {
    deviceUID: UID
    channelName: string
    name: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()
const router = useRouter()

const channelLabel =
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)
        ?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName

const saveSetting = async () => {
    const newProfile = new Profile(props.name, ProfileType.Default)
    settingsStore.profiles.push(newProfile)
    await settingsStore.saveProfile(newProfile.uid)
    emitter.emit('profile-add-menu', { profileUID: newProfile.uid })
    const setting = new DeviceSettingWriteProfileDTO(newProfile.uid)
    await settingsStore.saveDaemonDeviceSettingProfile(props.deviceUID, props.channelName, setting)
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('components.wizards.fanControl.profileCreatedApplied'),
        life: 3000,
    })
    emit('close')
    await router.push({
        name: 'device-speed',
        params: { deviceUID: props.deviceUID, channelName: props.channelName },
        query: { key: uuidV4() },
    })
}
</script>

<template>
    <div class="flex flex-col gap-y-4 w-96">
        <div class="w-full text-lg">
            {{ t('components.wizards.fanControl.newDefaultProfile') }}:
            <span class="font-bold">{{ props.name }}</span
            ><br />
            {{ t('components.wizards.fanControl.willCreatedAndAppliedTo') }}
            <span class="font-bold">{{ channelLabel }}</span
            >.
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 h-[2.375rem]" label="Back" @click="emit('nextStep', 3)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                :label="t('common.apply')"
                v-tooltip.bottom="t('views.speed.applySetting')"
                @click="saveSetting"
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
