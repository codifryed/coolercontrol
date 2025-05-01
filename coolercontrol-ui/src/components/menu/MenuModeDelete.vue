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
import { mdiDeleteOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useI18n } from 'vue-i18n'

interface Props {
    modeUID: UID
}

const emit = defineEmits<{
    (e: 'deleted', modeUID: UID): void
}>()

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const { t } = useI18n()

const deleteMode = (): void => {
    const modeUIDToDelete: UID = props.modeUID
    const modeToDelete = settingsStore.modes.find((mode) => mode.uid === modeUIDToDelete)!

    confirm.require({
        message: t('views.modes.deleteModeConfirm', {
            name: modeToDelete.name,
        }),
        header: t('views.modes.deleteMode'),
        icon: 'pi pi-exclamation-triangle',
        accept: async () => {
            await settingsStore.deleteMode(modeUIDToDelete)
            emit('deleted', modeUIDToDelete)
        },
    })
}
</script>

<template>
    <div v-tooltip.top="{ value: t('layout.menu.tooltips.deleteMode') }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="deleteMode"
        >
            <svg-icon type="mdi" :path="mdiDeleteOutline" :size="deviceStore.getREMSize(1.5)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
