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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiPlusBoxMultipleOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { inject } from 'vue'
import { Emitter, EventType } from 'mitt'
import { UID } from '@/models/Device.ts'
import { Profile, ProfileType } from '@/models/Profile.ts'
import { useToast } from 'primevue/usetoast'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

interface Props {}

defineProps<Props>()
const emit = defineEmits<{
    (e: 'added', profileUID: UID): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
const router = useRouter()

const addProfile = async (): Promise<void> => {
    const newProfile = new Profile(t('views.profiles.newProfile'), ProfileType.Default)
    settingsStore.profiles.push(newProfile)
    await settingsStore.saveProfile(newProfile.uid)
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('views.profiles.createProfile'),
        life: 3000,
    })
    emit('added', newProfile.uid)
    await router.push({ name: 'profiles', params: { profileUID: newProfile.uid } })
}
// be able to add a profile from the side menu add button:
emitter.on('profile-add', addProfile)
</script>

<template>
    <div v-tooltip.top="{ value: t('layout.menu.tooltips.addProfile') }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="addProfile"
        >
            <svg-icon
                type="mdi"
                :path="mdiPlusBoxMultipleOutline"
                :size="deviceStore.getREMSize(1.5)"
            />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
