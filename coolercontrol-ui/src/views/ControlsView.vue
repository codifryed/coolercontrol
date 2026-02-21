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
import { nextTick, provide, ref } from 'vue'
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import { useOverviewGraph } from '@/components/control-flow/useOverviewGraph'
import FanChannelNode from '@/components/control-flow/FanChannelNode.vue'
import LcdChannelNode from '@/components/control-flow/LcdChannelNode.vue'
import LightingChannelNode from '@/components/control-flow/LightingChannelNode.vue'
import DeviceLabelNode from '@/components/control-flow/DeviceLabelNode.vue'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiHelpCircleOutline } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useI18n } from 'vue-i18n'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const colorStore = useThemeColorsStore()

provide('flowViewMode', 'overview')

const { nodes } = useOverviewGraph()

const MIN_ZOOM = 1.2
const lockedZoom = ref(MIN_ZOOM)
const FAN_NODE_WIDTH = 220
const LABEL_COL_GAP = 320
const H_PADDING = deviceStore.getREMSize(1)

const { onPaneReady, setViewport } = useVueFlow('control-flow-overview')

onPaneReady(() => {
    nextTick(() => fitToWidth())
})

function fitToWidth() {
    let maxRight = 0
    for (const node of nodes.value) {
        let nodeWidth: number
        if (node.type === 'deviceLabel') {
            const cols = Math.min(node.data.channelCount, 3)
            nodeWidth = Math.max(FAN_NODE_WIDTH, (cols - 1) * LABEL_COL_GAP + FAN_NODE_WIDTH)
        } else {
            nodeWidth = FAN_NODE_WIDTH
        }
        maxRight = Math.max(maxRight, node.position.x + nodeWidth)
    }
    if (maxRight === 0) return

    // const vpWidth = dimensions.value.width
    // const zoom = Math.max((vpWidth - H_PADDING * 2) / maxRight, MIN_ZOOM)
    // lockedZoom.value = zoom
    // setViewport({ x: H_PADDING, y: H_PADDING, zoom })
    setViewport({ x: H_PADDING, y: H_PADDING, zoom: lockedZoom.value })
}
</script>

<template>
    <div class="flex h-full flex-col">
        <div class="flex items-center justify-between border-b-4 border-border-one px-4 py-2">
            <span class="text-2xl font-bold text-text-color">System Controls</span>
            <div class="flex items-center gap-x-1 text-sm text-text-color-secondary">
                <svg-icon
                    type="mdi"
                    class="inline"
                    :path="mdiHelpCircleOutline"
                    :size="deviceStore.getREMSize(1.3)"
                />
                {{ t('layout.settings.devices.detectionIssues') }}
                <a
                    target="_blank"
                    href="https://docs.coolercontrol.org/hardware-support.html"
                    class="text-accent"
                >
                    {{ t('layout.settings.devices.hardwareSupportDoc') }}
                </a>
            </div>
        </div>

        <div v-if="nodes.length === 0" class="flex flex-1 items-center justify-center">
            <div class="text-center text-text-color-secondary">
                <p class="text-lg">No controllable channels found.</p>
                <p class="mt-2">
                    Check the
                    <a
                        target="_blank"
                        href="https://docs.coolercontrol.org/hardware-support.html"
                        class="text-accent"
                    >
                        hardware support documentation
                    </a>
                    for details.
                </p>
            </div>
        </div>

        <VueFlow
            v-else
            id="control-flow-overview"
            :nodes="nodes"
            :edges="[]"
            :nodes-draggable="false"
            :nodes-connectable="false"
            :elements-selectable="false"
            :pan-on-drag="false"
            pan-on-scroll
            :zoom-on-scroll="false"
            :zoom-on-pinch="false"
            :zoom-on-double-click="false"
            :min-zoom="lockedZoom"
            :max-zoom="lockedZoom"
            class="flex-1"
        >
            <template #node-fanChannel="fanProps">
                <FanChannelNode v-bind="fanProps" />
            </template>
            <template #node-lcdChannel="lcdProps">
                <LcdChannelNode v-bind="lcdProps" />
            </template>
            <template #node-lightingChannel="lightingProps">
                <LightingChannelNode v-bind="lightingProps" />
            </template>
            <template #node-deviceLabel="labelProps">
                <DeviceLabelNode v-bind="labelProps" />
            </template>
            <Background
                variant="lines"
                :pattern-color="colorStore.rgbToHex(colorStore.themeColors.border)"
                :line-width="1"
                :gap="deviceStore.getREMSize(6)"
            />
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
