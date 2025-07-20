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
import { mdiPlusBoxMultipleOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { defineAsyncComponent, inject } from 'vue'
import { Emitter, EventType } from 'mitt'
import { useI18n } from 'vue-i18n'
import { useDialog } from 'primevue/usedialog'

interface Props {}

defineProps<Props>()
// const emit = defineEmits<{
//     (e: 'added', profileUID: UID): void
// }>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const dialog = useDialog()
const profileWizard = defineAsyncComponent(() => import('../wizards/profile/Wizard.vue'))

const addProfile = async (): Promise<void> => {
    dialog.open(profileWizard, {
        props: {
            header: t('views.profiles.newProfile'),
            position: 'center',
            modal: true,
            dismissableMask: true,
        },
        data: {},
    })
    // Now handled by the New Profile Wizard:
    // const newProfile = new Profile(t('views.profiles.newProfile'), ProfileType.Default)
    // settingsStore.profiles.push(newProfile)
    // await settingsStore.saveProfile(newProfile.uid)
    // toast.add({
    //     severity: 'success',
    //     summary: t('common.success'),
    //     detail: t('views.profiles.createProfile'),
    //     life: 3000,
    // })
    // emit('added', newProfile.uid)
    // await router.push({ name: 'profiles', params: { profileUID: newProfile.uid } })
}
// to be able to add a profile from the side menu add button:
emitter.on('profile-add', addProfile)
</script>

<template>
    <div
        class="rounded-lg w-8 h-8 border-none p-0 text-text-color-secondary outline-0 text-center justify-center items-center flex hover:text-text-color hover:bg-surface-hover"
        v-tooltip.top="{ value: t('layout.menu.tooltips.addProfile') }"
    >
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color outline-0"
            @click.stop="addProfile"
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
