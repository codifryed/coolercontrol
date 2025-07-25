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
import { mdiBookmarkCheckOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useI18n } from 'vue-i18n'
import { UID } from '@/models/Device.ts'

const props = defineProps<{
    modeUID: UID
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()

const activateMode = async (): Promise<void> => {
    // auto-reloads active modes from the new active mode SSE (and triggers menu update)
    await settingsStore.activateMode(props.modeUID)
}
</script>

<template>
    <div
        v-tooltip.top="{
            value: t('views.mode.activateMode'),
            disabled: props.modeUID === settingsStore.modeActiveCurrent,
        }"
    >
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            :disabled="props.modeUID === settingsStore.modeActiveCurrent"
            @click.stop.prevent="activateMode"
        >
            <svg-icon
                type="mdi"
                :path="mdiBookmarkCheckOutline"
                :size="deviceStore.getREMSize(1.5)"
            />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
