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
import Select from 'primevue/select'
import { UID } from '@/models/Device.ts'
import { Profile } from '@/models/Profile.ts'
import { ref, Ref } from 'vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import Button from 'primevue/button'
import { v4 as uuidV4 } from 'uuid'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings.ts'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

interface Props {
    deviceUID: UID
    channelName: string
    selectedProfileUID: UID
}

const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()
const props = defineProps<Props>()

const { t } = useI18n()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const router = useRouter()

const selectedProfile: Ref<Profile> = ref(
    settingsStore.profiles.find((profile) => profile.uid === props.selectedProfileUID)!,
)

const getProfileOptions = (): any[] => settingsStore.profiles

const saveSetting = async () => {
    const setting = new DeviceSettingWriteProfileDTO(selectedProfile.value.uid)
    await settingsStore.saveDaemonDeviceSettingProfile(props.deviceUID, props.channelName, setting)
    await router.push({
        name: 'device-speed',
        params: { deviceUID: props.deviceUID, channelName: props.channelName },
        query: { key: uuidV4() },
    })
    emit('close')
}
</script>

<template>
    <div class="flex flex-col gap-y-4 w-72">
        <div class="mt-0 flex flex-col">
            <small class="ml-2 mb-1 font-light text-sm text-text-color-secondary">
                {{ t('components.wizards.fanControl.existingProfile') }}:
            </small>
            <Select
                v-model="selectedProfile"
                :options="getProfileOptions()"
                option-label="name"
                placeholder="Profile"
                class="w-full mr-4 h-full bg-bg-two"
                checkmark
                dropdown-icon="pi pi-chart-line"
                scroll-height="40rem"
            />
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 h-[2.375rem]" label="Back" @click="emit('nextStep', 1)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                label="Apply"
                v-tooltip.bottom="'Apply'"
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
