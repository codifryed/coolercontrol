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
import { Handle, Position } from '@vue-flow/core'
import type { NodeProps } from '@vue-flow/core'
import type { FanNodeData } from './useControlFlowGraph'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useRouter } from 'vue-router'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiFan } from '@mdi/js'

const props = defineProps<NodeProps<FanNodeData>>()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const router = useRouter()
const flowViewMode = inject<string>('flowViewMode', 'detail')

const profileName = computed(() => {
    if (props.data.isManual || !props.data.profileUID) return undefined
    if (props.data.profileUID === '0') return 'Default'
    return settingsStore.profiles.find((p) => p.uid === props.data.profileUID)?.name
})

const liveValues = computed(() => {
    const status = deviceStore.currentDeviceStatus.get(props.data.deviceUID)
    const channelValues = status?.get(props.data.channelName)
    return {
        duty: channelValues?.duty,
        rpm: channelValues?.rpm,
    }
})

function onClick() {
    if (flowViewMode === 'overview') {
        if (props.data.isManual || !props.data.profileUID || props.data.profileUID === '0') {
            router.push({
                name: 'device-speed',
                params: {
                    deviceUID: props.data.deviceUID,
                    channelName: props.data.channelName,
                },
            })
        } else {
            router.push({
                name: 'channel-control-flow',
                params: {
                    deviceUID: props.data.deviceUID,
                    channelName: props.data.channelName,
                },
            })
        }
    } else {
        router.push({
            name: 'device-speed',
            params: {
                deviceUID: props.data.deviceUID,
                channelName: props.data.channelName,
            },
        })
    }
}
</script>

<template>
    <div
        class="cursor-pointer rounded-lg border border-border-one bg-bg-two shadow-md transition-shadow hover:shadow-lg"
        style="min-width: 220px"
        @click="onClick"
    >
        <div
            class="flex items-center gap-2 rounded-t-lg px-3 py-2"
            :style="{ borderTop: `3px solid ${data.channelColor}` }"
        >
            <svg-icon type="mdi" :path="mdiFan" class="size-5 text-text-color" />
            <div class="flex-1 truncate text-sm font-semibold text-text-color">
                {{ data.channelLabel }}
            </div>
        </div>
        <div class="space-y-1 px-3 pb-2">
            <div class="truncate text-xs text-text-color-secondary">
                {{ data.deviceLabel }}
            </div>
            <div v-if="profileName" class="truncate text-xs text-accent">
                {{ profileName }}
            </div>
            <div class="flex items-center gap-3 text-xs">
                <template v-if="data.isManual">
                    <span class="rounded bg-warning/20 px-1.5 py-0.5 font-medium text-warning">
                        Manual: {{ data.manualDuty }}%
                    </span>
                </template>
                <template v-else>
                    <span v-if="liveValues.duty" class="font-medium text-text-color">
                        {{ liveValues.duty }}%
                    </span>
                    <span v-if="liveValues.rpm" class="text-text-color-secondary">
                        {{ liveValues.rpm }} RPM
                    </span>
                </template>
            </div>
        </div>
        <Handle type="target" :position="Position.Right" class="!bg-accent" />
    </div>
</template>
