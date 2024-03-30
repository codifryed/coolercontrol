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
import { ref } from 'vue'
import { Profile } from '@/models/Profile'
import Menu from 'primevue/menu'
import Button from 'primevue/button'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'

interface Props {
    profile: Profile
}

const props = defineProps<Props>()
const emit = defineEmits<{
    delete: []
}>()
const settingsStore = useSettingsStore()
const optionsMenu = ref()
const confirm = useConfirm()
const toast = useToast()

const optionsToggle = (event: any) => {
    optionsMenu.value.toggle(event)
}

const duplicateProfile = (profileToDuplicate: Profile): void => {
    const newProfile = new Profile(
        `${profileToDuplicate.name} (copy)`,
        profileToDuplicate.p_type,
        profileToDuplicate.speed_fixed,
        profileToDuplicate.temp_source,
        profileToDuplicate.speed_profile,
        profileToDuplicate.member_profile_uids,
        profileToDuplicate.mix_function_type,
    )
    settingsStore.profiles.push(newProfile)
    settingsStore.saveProfile(newProfile.uid)
    toast.add({
        severity: 'success',
        summary: 'Success',
        detail: 'Profile successfully Duplicated',
        life: 3000,
    })
}

const deleteProfile = (profileToDelete: Profile): void => {
    if (profileToDelete.uid === '0') {
        return
    }
    const associatedChannelSettings: Array<string> = []
    for (const [deviceUID, setting] of settingsStore.allDaemonDeviceSettings) {
        for (const channel_setting of setting.settings.values()) {
            if (channel_setting.profile_uid === profileToDelete.uid) {
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
            ? `Are you sure you want to delete the profile: "${profileToDelete.name}"?`
            : `This Profile is currently being used by: ${associatedChannelSettings}.
      Deleting this Profile will reset those channels' settings. Are you sure you want to delete "${profileToDelete.name}"?`
    confirm.require({
        message: deleteMessage,
        header: 'Delete Profile',
        icon: 'pi pi-exclamation-triangle',
        position: 'top',
        accept: () => {
            settingsStore.profiles.splice(
                settingsStore.profiles.findIndex((profile) => profile.uid === props.profile.uid),
                1,
            )
            settingsStore.deleteProfile(props.profile.uid)
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Profile successfully Deleted',
                life: 3000,
            })
            emit('delete')
        },
        reject: () => {},
    })
}

const profileOptions = () => {
    return props.profile.uid !== '0' // the non-deletable default profile
        ? [
              {
                  label: 'Duplicate',
                  icon: 'pi pi-copy',
                  command: () => duplicateProfile(props.profile),
              },
              {
                  label: 'Delete',
                  icon: 'pi pi-trash',
                  command: () => deleteProfile(props.profile),
              },
          ]
        : []
}
</script>

<template>
    <div class="flex" v-if="props.profile.uid !== '0'">
        <Button
            aria-label="Profile Card Options"
            icon="pi pi-ellipsis-v"
            rounded
            text
            plain
            size="small"
            class="ml-auto p-3"
            aria-controls="options_layout"
            style="height: 0.1rem; width: 0.1rem; box-shadow: none"
            type="button"
            aria-haspopup="true"
            @click.stop.prevent="optionsToggle($event)"
        />
        <Menu ref="optionsMenu" id="options_layout" :model="profileOptions()" popup class="w-8rem">
            <template #item="{ label, props }">
                <a class="flex" v-bind="props.action">
                    <span v-bind="props.icon" /><span v-bind="props.label">{{ label }}</span>
                </a>
            </template>
        </Menu>
    </div>
</template>

<style scoped lang="scss"></style>
