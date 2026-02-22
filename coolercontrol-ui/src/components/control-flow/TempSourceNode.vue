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
import { useI18n } from 'vue-i18n'
import { Handle, Position } from '@vue-flow/core'
import type { NodeProps } from '@vue-flow/core'
import type { TempSourceNodeData } from './useControlFlowGraph'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useRouter } from 'vue-router'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiThermometer } from '@mdi/js'

const props = defineProps<NodeProps<TempSourceNodeData>>()
const { t } = useI18n()
const deviceStore = useDeviceStore()
const router = useRouter()

const liveTemp = computed(() => {
    const status = deviceStore.currentDeviceStatus.get(props.data.deviceUID)
    return status?.get(props.data.tempName)?.temp
})

function onClick() {
    router.push({
        name: 'single-dashboard',
        params: {
            deviceUID: props.data.deviceUID,
            channelName: props.data.tempName,
        },
    })
}
</script>

<template>
    <div
        class="cursor-pointer rounded-lg border border-border-one bg-bg-two shadow-md transition-shadow hover:shadow-lg"
        style="min-width: 200px"
        @click="onClick"
    >
        <div
            class="flex items-center gap-2 rounded-t-lg px-3 py-2"
            :style="{ borderTop: `3px solid ${data.tempColor}` }"
        >
            <svg-icon type="mdi" :path="mdiThermometer" class="size-5 text-text-color" />
            <div class="flex-1 truncate text-sm font-semibold text-text-color">
                {{ data.tempLabel }}
            </div>
        </div>
        <div class="space-y-1 px-3 pb-2">
            <div class="truncate text-xs text-text-color-secondary">
                {{ data.deviceLabel }}
            </div>
            <div v-if="liveTemp" class="text-lg font-bold text-text-color">
                {{ liveTemp }}{{ t('common.tempUnit') }}
            </div>
        </div>
        <Handle type="source" :position="Position.Left" class="!bg-accent" />
    </div>
</template>
