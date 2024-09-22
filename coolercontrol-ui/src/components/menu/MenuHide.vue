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
import { mdiEyeOffOutline, mdiEyeOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import type { UID } from '@/models/Device.ts'
import { computed } from 'vue'

interface Props {
    deviceUID: UID
    channelName: string
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const deviceChannelHidden = computed(
    (): boolean =>
        settingsStore.allUIDeviceSettings
            .get(props.deviceUID)
            ?.sensorsAndChannels.get(props.channelName)?.hide ?? false,
)

const tooltipLabel = (): string => (deviceChannelHidden.value ? 'Show' : 'Hide')

const toggleHide = (): void => {
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.get(props.channelName)!.hide = !deviceChannelHidden.value
}
</script>

<template>
    <div v-tooltip.top="{ value: tooltipLabel() }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="toggleHide"
        >
            <svg-icon
                type="mdi"
                :path="deviceChannelHidden ? mdiEyeOutline : mdiEyeOffOutline"
                :size="deviceStore.getREMSize(1.5)"
            />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
