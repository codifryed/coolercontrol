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
import { Handle, Position } from '@vue-flow/core'
import type { NodeProps } from '@vue-flow/core'
import type { ProfileNodeData } from './useControlFlowGraph'
import { ProfileType } from '@/models/Profile'
import { useRouter } from 'vue-router'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiFunction, mdiChartLine } from '@mdi/js'

const props = defineProps<NodeProps<ProfileNodeData>>()
const router = useRouter()

function onClickProfile() {
    if (props.data.isDefault) return
    router.push({
        name: 'profiles',
        params: { profileUID: props.data.profileUID },
    })
}

function onClickFunction(e: Event) {
    e.stopPropagation()
    if (props.data.functionUID) {
        router.push({
            name: 'functions',
            params: { functionUID: props.data.functionUID },
        })
    }
}

const typeBadgeClass: Record<string, string> = {
    [ProfileType.Default]: 'bg-info/20 text-info',
    [ProfileType.Fixed]: 'bg-success/20 text-success',
    [ProfileType.Graph]: 'bg-accent/20 text-accent',
    [ProfileType.Mix]: 'bg-pink/20 text-pink',
    [ProfileType.Overlay]: 'bg-warning/20 text-warning',
}
</script>

<template>
    <div
        class="rounded-lg border border-border-one bg-bg-two shadow-md transition-shadow hover:shadow-lg"
        :class="data.isDefault ? 'cursor-default' : 'cursor-pointer'"
        style="min-width: 220px"
        @click="onClickProfile"
    >
        <div class="flex items-center gap-2 rounded-t-lg border-t-[3px] border-accent px-3 py-2">
            <svg-icon type="mdi" :path="mdiChartLine" class="size-5 text-text-color" />
            <div class="flex-1 truncate text-sm font-semibold text-text-color">
                {{ data.profileName }}
            </div>
            <span
                class="rounded px-1.5 py-0.5 text-xs font-medium"
                :class="typeBadgeClass[data.profileType] ?? 'bg-info/20 text-info'"
            >
                {{ data.profileType }}
            </span>
        </div>
        <div class="space-y-1 px-3 pb-2">
            <div
                v-if="data.profileType === ProfileType.Fixed && data.speedFixed != null"
                class="text-xs text-text-color-secondary"
            >
                Fixed: {{ data.speedFixed }}%
            </div>
            <div
                v-if="data.profileType === ProfileType.Mix && data.mixFunctionType"
                class="text-xs text-text-color-secondary"
            >
                Mix: {{ data.mixFunctionType }}
            </div>
            <!-- Function sub-section -->
            <div
                v-if="data.functionName"
                class="mt-1 flex cursor-pointer items-center gap-1.5 rounded border border-border-one/50 px-2 py-1 transition-colors hover:bg-surface-hover"
                @click="onClickFunction"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiFunction"
                    class="size-3.5 text-text-color-secondary"
                />
                <span class="truncate text-xs text-text-color-secondary">
                    {{ data.functionName }}
                </span>
                <span class="rounded bg-info/15 px-1 py-0.5 text-[10px] font-medium text-info">
                    {{ data.functionType }}
                </span>
            </div>
        </div>
        <Handle type="source" :position="Position.Left" class="!bg-accent" />
        <Handle type="target" :position="Position.Right" class="!bg-accent" />
    </div>
</template>
