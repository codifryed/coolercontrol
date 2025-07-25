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
import { mdiDeleteOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'

interface Props {
    profileUID: UID
}

const emit = defineEmits<{
    (e: 'deleted', profileUID: UID): void
    (e: 'close'): void
}>()

const props = defineProps<Props>()

const { t } = useI18n()
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
            ? t('views.profiles.deleteProfileConfirm', { name: profileName })
            : t('views.profiles.deleteProfileWithChannelsConfirm', {
                  name: profileName,
                  channels: associatedChannelSettings.join(', '),
              })
    confirm.require({
        message: deleteMessage,
        header: t('views.profiles.deleteProfile'),
        icon: 'pi pi-exclamation-triangle',
        accept: async () => {
            // emit needs to happen first for Profiles, since they're re-loaded in by deleting
            emit('deleted', profileUIDToDelete)
            await settingsStore.deleteProfile(profileUIDToDelete)
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('views.profiles.profileDeleted'),
                life: 3000,
            })
            emit('close')
        },
        reject: () => {
            emit('close')
        },
    })
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click.stop.prevent="deleteProfile"
    >
        <svg-icon type="mdi" :path="mdiDeleteOutline" :size="deviceStore.getREMSize(1.5)" />
        <span class="ml-1.5">
            {{ t('common.delete') }}
        </span>
    </Button>
</template>

<style scoped lang="scss"></style>
