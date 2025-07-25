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
import { mdiContentDuplicate } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { Profile } from '@/models/Profile.ts'
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'

interface Props {
    profileUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'added', profileUID: UID): void
    (e: 'close'): void
}>()

const { t } = useI18n()
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
        `${profileToDuplicate.name} ${t('common.copy')}`,
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
        summary: t('common.success'),
        detail: t('views.profiles.profileDuplicated'),
        life: 3000,
    })
    emit('added', newProfile.uid)
    emit('close')
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click.stop.prevent="duplicateProfile"
    >
        <svg-icon
            type="mdi"
            class="outline-0 !cursor-pointer"
            :path="mdiContentDuplicate"
            :size="deviceStore.getREMSize(1.25)"
        />
        <span class="ml-1.5">
            {{ t('layout.menu.tooltips.duplicate') }}
        </span>
    </Button>
</template>

<style scoped lang="scss"></style>
