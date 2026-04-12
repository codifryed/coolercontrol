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
import { computed, defineAsyncComponent, nextTick, onMounted, provide, ref, shallowRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import { useControlFlowGraph } from '@/components/control-flow/useControlFlowGraph'
import FanChannelNode from '@/components/control-flow/FanChannelNode.vue'
import ProfileNode from '@/components/control-flow/ProfileNode.vue'
import TempSourceNode from '@/components/control-flow/TempSourceNode.vue'
import CustomSensorNode from '@/components/control-flow/CustomSensorNode.vue'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiArrowLeft } from '@mdi/js'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { Controls } from '@vue-flow/controls'
import Button from 'primevue/button'
import Drawer from 'primevue/drawer'
import type { NodeDrawerTarget } from '@/components/control-flow/types'
import { useRouter } from 'vue-router'

const props = defineProps<{
    deviceUID: string
    channelName: string
}>()

const { t } = useI18n()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const colorStore = useThemeColorsStore()
const router = useRouter()

provide('flowViewMode', 'detail')

const drawerVisible = ref(false)
const drawerComponent = shallowRef<ReturnType<typeof defineAsyncComponent> | null>(null)
const drawerProps = ref<Record<string, any>>({})
const drawerKey = ref(0)

const viewComponents: Record<string, ReturnType<typeof defineAsyncComponent>> = {
    'device-speed': defineAsyncComponent(() => import('@/views/SpeedView.vue')),
    profiles: defineAsyncComponent(() => import('@/views/ProfileView.vue')),
    functions: defineAsyncComponent(() => import('@/views/FunctionView.vue')),
    'custom-sensors': defineAsyncComponent(() => import('@/views/CustomSensorView.vue')),
    'single-dashboard': defineAsyncComponent(() => import('@/views/SingleDashboardView.vue')),
}

function openNodeDrawer(target: NodeDrawerTarget) {
    const comp = viewComponents[target.route]
    if (!comp) return
    drawerComponent.value = comp
    drawerProps.value = target.params
    drawerKey.value++
    drawerVisible.value = true
}

provide('openNodeDrawer', openNodeDrawer)

const flashEdges = ref(false)
function onProfileSwitched() {
    if (!settingsStore.eyeCandy) return
    flashEdges.value = true
    setTimeout(() => {
        flashEdges.value = false
    }, 1500)
}
provide('onProfileSwitched', onProfileSwitched)

const styledEdges = computed(() =>
    edges.value.map((e) => ({
        ...e,
        class: flashEdges.value ? 'flash' : '',
    })),
)

const MAX_ZOOM = 1.2
const DEFAULT_ZOOM = 1.0
const MIN_ZOOM = 0.4
const NODE_WIDTH = 260
const NODE_HEIGHT = 100
const H_PADDING = deviceStore.getREMSize(1)
const V_PADDING = deviceStore.getREMSize(4)

const selectedFanKey = ref(`${props.deviceUID}/${props.channelName}`)
const { nodes, edges, loadCustomSensors } = useControlFlowGraph(selectedFanKey)
const { onPaneReady, setViewport, dimensions, onMove } = useVueFlow('channel-control-flow')

onMounted(async () => {
    await loadCustomSensors()
})

onPaneReady(() => {
    nextTick(() => fitToContent())
})

function fitToContent() {
    if (nodes.value.length === 0) return

    let minX = Infinity
    let maxRight = 0
    let minY = Infinity
    let maxBottom = 0
    for (const node of nodes.value) {
        minX = Math.min(minX, node.position.x)
        maxRight = Math.max(maxRight, node.position.x + NODE_WIDTH)
        minY = Math.min(minY, node.position.y)
        maxBottom = Math.max(maxBottom, node.position.y + NODE_HEIGHT)
    }

    const contentWidth = maxRight - minX
    const contentHeight = maxBottom - minY
    if (contentWidth === 0 || contentHeight === 0) return

    // Clamp between DEFAULT_ZOOM and MAX_ZOOM. When the chain is too wide to fit at
    // DEFAULT_ZOOM, start at DEFAULT_ZOOM with the fan node (left side) visible — the user
    // can pan right to explore. This keeps nodes legible (~195px+ wide) even for
    // complex 5-column chains.
    const vpWidth = dimensions.value.width
    const zoom = Math.min(
        Math.max((vpWidth - H_PADDING * 2) / contentWidth, DEFAULT_ZOOM),
        MAX_ZOOM,
    )
    setViewport({
        x: H_PADDING - minX * zoom,
        y: V_PADDING - minY * zoom + V_PADDING,
        zoom,
    })
}

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
        maxY = Math.max(maxY, node.position.y + NODE_HEIGHT)
        colSet.add(node.position.x)
        rowsPerCol.set(node.position.x, (rowsPerCol.get(node.position.x) ?? 0) + 1)
    }
    const cols = colSet.size
    const maxRows = Math.max(...rowsPerCol.values())
    return { minX, maxX, minY, maxY, cols, maxRows }
})

// Clamp viewport so at least EXTENT_MARGIN screen-pixels of content remain visible
onMove(({ flowTransform }) => {
    const bounds = contentBounds.value
    if (!bounds) return
    const { x, y, zoom } = flowTransform
    const vpW = dimensions.value.width
    const vpH = dimensions.value.height
    const nodesWidth = NODE_WIDTH * bounds.cols
    const nodesHeight = NODE_HEIGHT * bounds.maxRows
    const nodeWidth = Math.min(nodesWidth, vpW)
    const nodeHeight = Math.min(nodesHeight, vpH)
    const widthWithXMaxOffset = nodeWidth - (nodesWidth > vpW ? H_PADDING * bounds.cols : H_PADDING)
    const heightWithYMinOffset = nodesHeight > vpH ? nodeHeight - V_PADDING : nodeHeight + V_PADDING
    const heightWithYMaxOffset = nodesHeight > vpH ? nodeHeight - V_PADDING : nodeHeight + V_PADDING
    const xMin = nodeWidth - bounds.maxX * zoom
    const xMax = vpW - widthWithXMaxOffset - bounds.minX * zoom
    const yMin = heightWithYMinOffset - bounds.maxY * zoom
    const yMax = vpH - heightWithYMaxOffset - bounds.minY * zoom
    const clampedX = Math.max(xMin, Math.min(xMax, x))
    const clampedY = Math.max(yMin, Math.min(yMax, y))
    if (clampedX !== x || clampedY !== y) {
        setViewport({ x: clampedX, y: clampedY, zoom })
    }
})

const channelLabel = computed(() => {
    const deviceSettings = settingsStore.allUIDeviceSettings.get(props.deviceUID)
    return deviceSettings?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName
})
</script>

<template>
    <div class="flex h-full flex-col">
        <div class="flex items-center gap-3 border-b-4 border-border-one px-4 py-2">
            <Button
                text
                rounded
                class="!p-1"
                v-tooltip.bottom="t('views.controls.backToOverview')"
                @click="router.push({ name: 'system-controls' })"
            >
                <svg-icon type="mdi" :path="mdiArrowLeft" :size="deviceStore.getREMSize(1.25)" />
            </Button>
            <span class="text-2xl font-bold text-text-color">
                {{ channelLabel }} - {{ t('views.controls.controlFlow') }}
            </span>
        </div>

        <div v-if="nodes.length === 0" class="flex flex-1 items-center justify-center">
            <div class="text-center text-text-color-secondary">
                <p class="text-lg">{{ t('views.controls.noControlChain') }}</p>
            </div>
        </div>

        <VueFlow
            v-else
            id="channel-control-flow"
            :nodes="nodes"
            :edges="styledEdges"
            :default-viewport="{ x: H_PADDING, y: V_PADDING * 2, zoom: DEFAULT_ZOOM }"
            :min-zoom="MIN_ZOOM"
            :max-zoom="MAX_ZOOM"
            :nodes-draggable="false"
            :nodes-connectable="false"
            :elements-selectable="false"
            pan-on-drag
            pan-on-scroll
            :zoom-on-scroll="false"
            :zoom-on-pinch="true"
            :zoom-on-double-click="false"
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
            <Controls :show-interactive="false" class="!bg-bg-two" />
        </VueFlow>

        <Drawer
            v-model:visible="drawerVisible"
            position="right"
            class="!w-[78vw] !bg-bg-one"
            :dismissable="true"
            :modal="true"
        >
            <template #container="{ closeCallback }">
                <div class="flex h-full flex-row">
                    <div
                        class="flex flex-col items-center border-r border-border-one bg-bg-two px-1 py-3"
                    >
                        <Button
                            type="button"
                            icon="pi pi-times"
                            rounded
                            text
                            class="!text-text-color-secondary hover:!text-text-color hover:!bg-bg-two !w-10 !h-full !items-start"
                            @click="closeCallback"
                        />
                    </div>
                    <div class="flex-1 bg-bg-one">
                        <Suspense v-if="drawerComponent">
                            <component
                                :is="drawerComponent"
                                v-bind="drawerProps"
                                :key="drawerKey"
                            />
                        </Suspense>
                    </div>
                </div>
            </template>
        </Drawer>
    </div>
</template>

<style>
@import '@vue-flow/core/dist/style.css';
@import '@vue-flow/controls/dist/style.css';
@import '@vue-flow/minimap/dist/style.css';

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

.vue-flow__controls-button {
    background-color: rgb(var(--colors-bg-two));
    border-color: rgb(var(--colors-bg-two));
    color: rgb(var(--colors-text-color-secondary));
}

.vue-flow__controls-button svg {
    fill: rgb(var(--colors-text-color-secondary));
}

.vue-flow__controls-button:hover {
    background-color: rgba(var(--colors-surface-hover) / 0.05);
    border-color: rgb(var(--colors-bg-two));
    color: rgb(var(--colors-text-color-secondary));
}

.vue-flow__controls-button:hover svg {
    fill: rgb(var(--colors-text-color));
}

.vue-flow__minimap {
    background-color: rgb(var(--colors-bg-one));
}

@keyframes edge-flash {
    0%,
    100% {
        stroke: rgb(var(--colors-accent));
        stroke-width: 1.5;
        filter: drop-shadow(0 0 0 transparent);
    }
    15%,
    55% {
        stroke: rgb(var(--colors-accent));
        stroke-width: 5;
        filter: drop-shadow(0 0 6px rgb(var(--colors-accent)));
    }
    35%,
    75% {
        stroke: rgb(var(--colors-accent));
        stroke-width: 2.5;
        filter: drop-shadow(0 0 2px rgb(var(--colors-accent)));
    }
}

.vue-flow__edge.flash path {
    animation: edge-flash 1.5s ease-in-out;
}
</style>
