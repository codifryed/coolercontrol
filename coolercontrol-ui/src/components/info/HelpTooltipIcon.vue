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
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiHelpCircleOutline } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore.ts'

// Component boundary with primitive props so an open tooltip survives parent
// re-renders (see MenuHealthIcon.vue); the Info page updates on every log line.
interface Props {
    tooltip: string
}
const props = defineProps<Props>()
const deviceStore = useDeviceStore()
</script>

<template>
    <svg-icon
        v-tooltip.top="{ escape: false, value: props.tooltip }"
        type="mdi"
        class="text-warning cursor-help"
        :path="mdiHelpCircleOutline"
        :size="deviceStore.getREMSize(1.25)"
    />
</template>
