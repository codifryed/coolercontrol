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
import { mdiContentDuplicate } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { Profile } from '@/models/Profile.ts'
import { useToast } from 'primevue/usetoast'

interface Props {
    profileUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'added', profileUID: UID): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()

const duplicateProfile = async (): Promise<void> => {
    const profileToDuplicate = settingsStore.profiles.find(
        (profile) => profile.uid === props.profileUID,
    )
    if (profileToDuplicate == null) {
        console.error('Profile not found for duplication: ' + props.profileUID)
        return
    }
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
    await settingsStore.saveProfile(newProfile.uid)
    toast.add({
        severity: 'success',
        summary: 'Success',
        detail: 'Profile Duplicated',
        life: 3000,
    })
    emit('added', newProfile.uid)
}
</script>

<template>
    <div v-tooltip.top="{ value: 'Duplicate' }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="duplicateProfile"
        >
            <svg-icon type="mdi" :path="mdiContentDuplicate" :size="deviceStore.getREMSize(1.2)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
