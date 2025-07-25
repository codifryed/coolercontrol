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
import { mdiContentDuplicate } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { useI18n } from 'vue-i18n'

interface Props {
    modeUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'added', modeUID: UID): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()

const duplicateMode = async (): Promise<void> => {
    const modeToDuplicate = settingsStore.modes.find((mode) => mode.uid === props.modeUID)
    if (modeToDuplicate == null) {
        console.error('Mode not found for duplication: ' + props.modeUID)
        return
    }
    const newMode = await settingsStore.duplicateMode(props.modeUID)
    if (newMode == null) return
    emit('added', newMode.uid)
}
</script>

<template>
    <div v-tooltip.top="{ value: t('layout.menu.tooltips.duplicate') }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="duplicateMode"
        >
            <svg-icon type="mdi" :path="mdiContentDuplicate" :size="deviceStore.getREMSize(1.2)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
