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
import { computed, onMounted, onUnmounted, provide, ref } from 'vue'
import { PanOnScrollMode, useVueFlow, VueFlow } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import {
    COL_GAP,
    NODE_WIDTH,
    ROW_GAP,
    useOverviewGraph,
} from '@/components/control-flow/useOverviewGraph'
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

const lockedZoom = ref(1.2)
const H_PADDING = deviceStore.getREMSize(1)

const containerRef = ref<HTMLElement | null>(null)
const containerWidth = ref(0)

const columnsPerRow = computed(() => {
    const availableFlowWidth = containerWidth.value / lockedZoom.value
    if (availableFlowWidth <= 0) return 3
    const cols = Math.floor((availableFlowWidth - 2 * H_PADDING - NODE_WIDTH) / COL_GAP) + 1
    return Math.max(1, cols)
})

const { nodes } = useOverviewGraph(columnsPerRow)

const { onMove, setViewport, dimensions } = useVueFlow('control-flow-overview')

const contentBounds = computed(() => {
    if (nodes.value.length === 0) return null
    let minX = Infinity
    let maxX = -Infinity
    let minY = Infinity
    let maxY = -Infinity
    const colSet = new Set<number>()
    const rowsPerCol = new Map<number, number>()
    for (const node of nodes.value) {
        minX = Math.min(minX, node.position.x)
        maxX = Math.max(maxX, node.position.x + NODE_WIDTH)
        minY = Math.min(minY, node.position.y)
        maxY = Math.max(maxY, node.position.y + ROW_GAP)
        colSet.add(node.position.x)
        rowsPerCol.set(node.position.x, (rowsPerCol.get(node.position.x) ?? 0) + 1)
    }
    const cols = colSet.size
    const maxRows = Math.max(...rowsPerCol.values())
    return { minX, maxX, minY, maxY, cols, maxRows }
})

// Clamp panning so content stays visible, scaled by node count
onMove(({ flowTransform }) => {
    const bounds = contentBounds.value
    if (!bounds) return
    const { x, y, zoom } = flowTransform
    const vpW = dimensions.value.width
    const vpH = dimensions.value.height
    const nodesWidth = NODE_WIDTH * bounds.cols
    const nodesHeight = ROW_GAP * bounds.maxRows
    const nodeWidth = Math.min(nodesWidth, vpW)
    const nodeHeight = Math.min(nodesHeight, vpH)
    const xMargin = nodeWidth - (nodesWidth > vpW ? H_PADDING * bounds.cols : H_PADDING)
    const yMargin = nodesHeight > vpH ? nodeHeight - H_PADDING : nodeHeight + H_PADDING
    const xMin = nodeWidth - bounds.maxX * zoom
    const xMax = vpW - xMargin - bounds.minX * zoom
    const yMin = yMargin - bounds.maxY * zoom
    const yMax = vpH - yMargin - bounds.minY * zoom
    const clampedX = Math.max(xMin, Math.min(xMax, x))
    const clampedY = Math.max(yMin, Math.min(yMax, y))
    if (clampedX !== x || clampedY !== y) {
        setViewport({ x: clampedX, y: clampedY, zoom })
    }
})

let resizeObserver: ResizeObserver | undefined
onMounted(() => {
    if (!containerRef.value) return
    resizeObserver = new ResizeObserver((entries) => {
        for (const entry of entries) {
            containerWidth.value = entry.contentRect.width
        }
    })
    resizeObserver.observe(containerRef.value)
})
onUnmounted(() => {
    resizeObserver?.disconnect()
})
</script>

<template>
    <div ref="containerRef" class="flex h-full flex-col">
        <div class="flex items-center justify-between border-b-4 border-border-one px-4 py-2">
            <span class="text-2xl font-bold text-text-color">{{ t('views.controls.title') }}</span>
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
                <p class="text-lg">{{ t('views.controls.noControllableChannels') }}</p>
                <p class="mt-2">
                    {{ t('layout.settings.devices.detectionIssues') }}
                    <a
                        target="_blank"
                        href="https://docs.coolercontrol.org/hardware-support.html"
                        class="text-accent"
                    >
                        {{ t('layout.settings.devices.hardwareSupportDoc') }}
                    </a>
                </p>
            </div>
        </div>

        <VueFlow
            v-else
            id="control-flow-overview"
            :nodes="nodes"
            :edges="[]"
            :default-viewport="{ x: H_PADDING, y: H_PADDING, zoom: lockedZoom }"
            :nodes-draggable="false"
            :nodes-connectable="false"
            :elements-selectable="false"
            :pan-on-drag="false"
            pan-on-scroll
            :pan-on-scroll-mode="PanOnScrollMode.Vertical"
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
