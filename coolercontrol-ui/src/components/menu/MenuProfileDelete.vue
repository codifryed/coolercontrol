<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2024  Guy Boldon and contributors
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
import { mdiDeleteOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'

interface Props {
    profileUID: UID
}

const emit = defineEmits<{
    (e: 'deleted', functionUID: UID): void
}>()

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()

const deleteProfile = (): void => {
    const profileUIDToDelete: UID = props.profileUID
    const profileIndex: number = settingsStore.profiles.findIndex(
        (profile) => profile.uid === profileUIDToDelete,
    )
    if (profileIndex === -1) {
        console.error('Profile not found for removal: ' + profileUIDToDelete)
        return
    }
    if (profileUIDToDelete === '0') {
        return // can't delete default
    }
    const profileName = settingsStore.profiles[profileIndex].name
    const associatedChannelSettings: Array<string> = []
    for (const [deviceUID, setting] of settingsStore.allDaemonDeviceSettings) {
        for (const channel_setting of setting.settings.values()) {
            if (channel_setting.profile_uid === profileUIDToDelete) {
                associatedChannelSettings.push(
                    settingsStore.allUIDeviceSettings
                        .get(deviceUID)!
                        .sensorsAndChannels.get(channel_setting.channel_name)!.name,
                )
            }
        }
    }
    const deleteMessage: string =
        associatedChannelSettings.length === 0
            ? `Are you sure you want to delete: "${profileName}"?`
            : `"${profileName}" is currently being used by: ${associatedChannelSettings}.
                Deleting this Profile will reset those channels' settings.
                Are you sure you want to delete "${profileName}"?`
    confirm.require({
        message: deleteMessage,
        header: 'Delete Profile',
        icon: 'pi pi-exclamation-triangle',
        accept: async () => {
            settingsStore.profiles.splice(profileIndex, 1)
            await settingsStore.deleteProfile(profileUIDToDelete)
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Profile Deleted',
                life: 3000,
            })
            emit('deleted', profileUIDToDelete)
        },
    })
}
</script>

<template>
    <div v-tooltip.top="{ value: 'Delete' }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="deleteProfile"
        >
            <svg-icon type="mdi" :path="mdiDeleteOutline" :size="deviceStore.getREMSize(1.5)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
