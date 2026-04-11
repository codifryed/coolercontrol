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
import { computed, inject } from 'vue'
import { useI18n } from 'vue-i18n'
import { Handle, Position } from '@vue-flow/core'
import type { NodeProps } from '@vue-flow/core'
import type { CustomSensorNodeData } from './useControlFlowGraph'
import type { NodeDrawerTarget } from './types'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useRouter } from 'vue-router'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiFlaskRoundBottom, mdiSwapHorizontal } from '@mdi/js'

const props = defineProps<NodeProps<CustomSensorNodeData>>()
const { t } = useI18n()
const deviceStore = useDeviceStore()
const router = useRouter()
const flowViewMode = inject<string>('flowViewMode', 'detail')
const openNodeDrawer = inject<(target: NodeDrawerTarget) => void>('openNodeDrawer')

const showSwapButton = computed(() => flowViewMode === 'detail')

function onSwapClick(event: Event) {
    event.stopPropagation()
    onClick()
}

const liveTemp = computed(() => {
    const status = deviceStore.currentDeviceStatus.get(props.data.deviceUID)
    return status?.get(props.data.sensorId)?.temp
})

function onClick() {
    const target = {
        route: 'custom-sensors',
        params: { customSensorID: props.data.sensorId },
    }
    if (openNodeDrawer) {
        openNodeDrawer(target)
    } else {
        router.push({ name: target.route, params: target.params })
    }
}

const typeBadgeClass: Record<string, string> = {
    Mix: 'bg-pink/20 text-pink',
    File: 'bg-success/20 text-success',
    Offset: 'bg-warning/20 text-warning',
}
</script>

<template>
    <div
        class="group/node cursor-pointer rounded-lg border border-border-one bg-bg-two shadow-md transition-all hover:shadow-lg hover:border-accent"
        style="min-width: 200px"
        @click="onClick"
    >
        <div class="flex items-center gap-2 rounded-t-lg border-t-[3px] border-green px-3 py-2">
            <svg-icon type="mdi" :path="mdiFlaskRoundBottom" class="size-5 text-text-color" />
            <div class="flex-1 truncate text-sm font-semibold text-text-color">
                {{ data.sensorName }}
            </div>
            <span
                class="rounded px-1.5 py-0.5 text-xs font-medium"
                :class="typeBadgeClass[data.csType] ?? 'bg-info/20 text-info'"
            >
                {{ data.csType }}
            </span>
            <div
                v-if="showSwapButton"
                v-tooltip.top="t('views.controls.editSources')"
                class="flex size-8 items-center justify-center rounded-md transition-all hover:bg-accent/15"
                @click="onSwapClick"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiSwapHorizontal"
                    class="size-5 text-text-color transition-colors hover:text-accent"
                />
            </div>
        </div>
        <div class="px-3 pb-2">
            <div v-if="liveTemp" class="text-lg font-bold text-text-color">
                {{ liveTemp }}{{ t('common.tempUnit') }}
            </div>
        </div>
        <Handle type="source" :position="Position.Left" class="!bg-accent" />
        <Handle type="target" :position="Position.Right" class="!bg-accent" />
    </div>
</template>
