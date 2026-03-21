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
import type { FanNodeData } from './useControlFlowGraph'
import type { NodeDrawerTarget } from './types'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useRouter } from 'vue-router'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiFan, mdiChartLine, mdiThermometer, mdiChevronRight } from '@mdi/js'

const props = defineProps<NodeProps<FanNodeData>>()
const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const router = useRouter()
const flowViewMode = inject<string>('flowViewMode', 'detail')
const openNodeDrawer = inject<((target: NodeDrawerTarget) => void) | undefined>(
    'openNodeDrawer',
    undefined,
)

const profileName = computed(() => {
    if (props.data.isManual || !props.data.profileUID) return undefined
    if (props.data.profileUID === '0') return t('models.profile.profileType.default')
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

function stepIcon(type: string): string {
    return type === 'tempSource' ? mdiThermometer : mdiChartLine
}

function onClick() {
    const speedTarget = {
        route: 'device-speed',
        params: {
            deviceUID: props.data.deviceUID,
            channelName: props.data.channelName,
        },
    }
    if (flowViewMode === 'overview') {
        router.push({
            name: 'channel-control-flow',
            params: {
                deviceUID: props.data.deviceUID,
                channelName: props.data.channelName,
            },
        })
    } else if (openNodeDrawer) {
        openNodeDrawer(speedTarget)
    } else {
        router.push({ name: speedTarget.route, params: speedTarget.params })
    }
}
</script>

<template>
    <div
        class="cursor-pointer rounded-lg border border-border-one bg-bg-two shadow-md transition-shadow hover:shadow-lg"
        :style="{ width: flowViewMode === 'overview' ? '220px' : undefined, minWidth: '220px' }"
        @click="onClick"
    >
        <div
            class="flex items-center gap-2 rounded-t-lg px-3 py-2"
            :style="{ borderTop: `3px solid ${data.channelColor}` }"
        >
            <svg-icon
                type="mdi"
                :path="mdiFan"
                class="size-5 text-text-color"
                :class="{
                    'animate-spin-slow':
                        settingsStore.eyeCandy &&
                        (liveValues.rpm != null
                            ? Number(liveValues.rpm) > 0
                            : Number(liveValues.duty ?? 0) > 0),
                }"
            />
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
                <span
                    v-if="data.isManual"
                    class="rounded bg-warning/20 px-1.5 py-0.5 font-medium text-warning"
                >
                    {{ t('views.speed.manual') }}: {{ data.manualDuty
                    }}{{ t('common.percentUnit') }}
                </span>
                <span v-if="liveValues.duty" class="font-medium text-text-color">
                    {{ liveValues.duty }}{{ t('common.percentUnit') }}
                </span>
                <span v-if="liveValues.rpm" class="text-text-color-secondary">
                    {{ liveValues.rpm }} {{ t('models.dataType.rpm') }}
                </span>
            </div>
        </div>
        <div
            v-if="flowViewMode === 'overview' && data.chainSummary?.hasChain"
            class="flex items-center gap-1 overflow-hidden border-t border-border-one px-3 py-1.5"
            v-tooltip.bottom="t('views.controls.viewControlFlow')"
        >
            <template v-for="(step, idx) in data.chainSummary.steps.slice(0, 3)" :key="idx">
                <svg-icon
                    v-if="idx > 0"
                    type="mdi"
                    :path="mdiChevronRight"
                    class="size-3 shrink-0 text-text-color-secondary"
                />
                <svg-icon
                    type="mdi"
                    :path="stepIcon(step.type)"
                    class="size-3 shrink-0"
                    :class="step.type === 'profile' ? 'text-accent' : 'text-text-color-secondary'"
                />
                <span
                    class="truncate text-[11px]"
                    :class="step.type === 'profile' ? 'text-accent' : 'text-text-color-secondary'"
                >
                    {{ step.name }}
                </span>
            </template>
            <svg-icon
                type="mdi"
                :path="mdiChevronRight"
                class="ml-auto size-3.5 shrink-0 text-text-color-secondary"
            />
        </div>
        <Handle type="target" :position="Position.Right" class="!bg-accent" />
    </div>
</template>
