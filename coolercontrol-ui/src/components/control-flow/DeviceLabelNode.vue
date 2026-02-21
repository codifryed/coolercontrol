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
import { computed } from 'vue'
import type { NodeProps } from '@vue-flow/core'
import type { DeviceLabelNodeData } from './useOverviewGraph'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiChip } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'

const FAN_NODE_WIDTH = 220
// const COL_GAP = 320

const props = defineProps<NodeProps<DeviceLabelNodeData>>()
const deviceStore = useDeviceStore()
const COL_GAP = deviceStore.getREMSize(20)

const labelWidth = computed(() => {
    const cols = Math.min(props.data.channelCount, 3)
    return Math.max(FAN_NODE_WIDTH, (cols - 1) * COL_GAP + FAN_NODE_WIDTH)
})
</script>

<template>
    <div
        class="flex cursor-default items-center gap-2 rounded-lg bg-bg-two px-4 py-2"
        :style="{
            borderLeft: `4px solid ${data.deviceColor}`,
            width: `${labelWidth}px`,
        }"
    >
        <svg-icon
            type="mdi"
            :path="mdiChip"
            :size="deviceStore.getREMSize(1.2)"
            class="text-text-color-secondary"
        />
        <span class="text-sm font-semibold text-text-color">
            {{ data.deviceName }}
        </span>
    </div>
</template>
