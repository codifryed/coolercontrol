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
import { computed, nextTick, onMounted, provide, ref } from 'vue'
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import { useControlFlowGraph } from '@/components/control-flow/useControlFlowGraph'
import FanChannelNode from '@/components/control-flow/FanChannelNode.vue'
import ProfileNode from '@/components/control-flow/ProfileNode.vue'
import TempSourceNode from '@/components/control-flow/TempSourceNode.vue'
import CustomSensorNode from '@/components/control-flow/CustomSensorNode.vue'
import FlowLegend from '@/components/control-flow/FlowLegend.vue'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'

const props = defineProps<{
    deviceUID: string
    channelName: string
}>()

const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const colorStore = useThemeColorsStore()

provide('flowViewMode', 'detail')

const MAX_ZOOM = 1.2
const MIN_ZOOM = 0.75
const NODE_WIDTH = 260
const H_PADDING = deviceStore.getREMSize(1)
const V_PADDING = deviceStore.getREMSize(4)

const selectedFanKey = ref(`${props.deviceUID}/${props.channelName}`)
const { nodes, edges, loadCustomSensors } = useControlFlowGraph(selectedFanKey)
const { onPaneReady, setViewport, dimensions } = useVueFlow('channel-control-flow')

onMounted(async () => {
    await loadCustomSensors()
})

onPaneReady(() => {
    nextTick(() => fitToWidth())
})

function fitToWidth() {
    if (nodes.value.length === 0) return

    let minX = Infinity
    let maxRight = 0
    let minY = Infinity
    for (const node of nodes.value) {
        minX = Math.min(minX, node.position.x)
        maxRight = Math.max(maxRight, node.position.x + NODE_WIDTH)
        minY = Math.min(minY, node.position.y)
    }

    const contentWidth = maxRight - minX
    if (contentWidth === 0) return

    // Clamp between MIN_ZOOM and MAX_ZOOM. When the chain is too wide to fit at
    // MIN_ZOOM, start at MIN_ZOOM with the fan node (left side) visible — the user
    // can pan right to explore. This keeps nodes legible (~195px+ wide) even for
    // complex 5-column chains.
    const vpWidth = dimensions.value.width
    const zoom = Math.min(Math.max((vpWidth - H_PADDING * 2) / contentWidth, MIN_ZOOM), MAX_ZOOM)
    setViewport({
        x: H_PADDING - minX * zoom,
        y: V_PADDING - minY * zoom + V_PADDING,
        zoom,
    })
}

const channelLabel = computed(() => {
    const deviceSettings = settingsStore.allUIDeviceSettings.get(props.deviceUID)
    return deviceSettings?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName
})
</script>

<template>
    <div class="flex h-full flex-col">
        <div class="flex items-center gap-3 border-b-4 border-border-one px-4 py-2">
            <span class="text-2xl font-bold text-text-color">
                {{ channelLabel }} - Control Flow
            </span>
        </div>

        <div v-if="nodes.length === 0" class="flex flex-1 items-center justify-center">
            <div class="text-center text-text-color-secondary">
                <p class="text-lg">No control chain found for this channel.</p>
            </div>
        </div>

        <VueFlow
            v-else
            id="channel-control-flow"
            :nodes="nodes"
            :edges="edges"
            :default-viewport="{ x: H_PADDING, y: V_PADDING * 2, zoom: MAX_ZOOM }"
            :nodes-draggable="false"
            :nodes-connectable="false"
            :elements-selectable="false"
            class="flex-1"
        >
            <template #node-fanChannel="fanProps">
                <FanChannelNode v-bind="fanProps" />
            </template>
            <template #node-profile="profileProps">
                <ProfileNode v-bind="profileProps" />
            </template>
            <template #node-tempSource="tempProps">
                <TempSourceNode v-bind="tempProps" />
            </template>
            <template #node-customSensor="csProps">
                <CustomSensorNode v-bind="csProps" />
            </template>
            <Background
                variant="lines"
                :pattern-color="colorStore.rgbToHex(colorStore.themeColors.border)"
                :line-width="1"
                :gap="deviceStore.getREMSize(6)"
            />
            <FlowLegend />
        </VueFlow>
    </div>
</template>

<style>
@import '@vue-flow/core/dist/style.css';

.vue-flow {
    background-color: rgb(var(--colors-bg-one));
}

.vue-flow__node {
    pointer-events: all !important;
}

.vue-flow__handle {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: none;
}
</style>
