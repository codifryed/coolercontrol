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
import type { UID } from '@/models/Device.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, ComputedRef } from 'vue'
import { mdiEyeOffOutline, mdiEyeOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'

interface Props {
    deviceUID: UID
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const allChannelsHidden: ComputedRef<boolean> = computed((): boolean => {
    for (const sensorChannel of settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.values()) {
        if (!sensorChannel.hide) return false
    }
    return true
})

const toggleAllChannels = (): void => {
    const startingValue = !allChannelsHidden.value
    for (const sensorChannel of settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.values()) {
        sensorChannel.hide = startingValue
    }
}

const tooltipLabel = (): string => (allChannelsHidden.value ? 'Show All' : 'Hide All')
</script>

<template>
    <div v-tooltip.top="{ value: tooltipLabel() }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="toggleAllChannels"
        >
            <svg-icon
                type="mdi"
                :path="allChannelsHidden ? mdiEyeOutline : mdiEyeOffOutline"
                :size="deviceStore.getREMSize(1.5)"
            />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
