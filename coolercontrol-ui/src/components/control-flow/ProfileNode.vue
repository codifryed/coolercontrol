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
import { computed, inject, ref } from 'vue'
import { Handle, Position } from '@vue-flow/core'
import type { NodeProps } from '@vue-flow/core'
import type { ProfileNodeData } from './useControlFlowGraph'
import type { NodeDrawerTarget } from './types'
import { ProfileType } from '@/models/Profile'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiFunction,
    mdiChartLine,
    mdiInformationSlabCircleOutline,
    mdiSwapHorizontal,
} from '@mdi/js'
import TempSourceSwitchPopover from './TempSourceSwitchPopover.vue'
import FunctionSwitchPopover from './FunctionSwitchPopover.vue'
import MemberProfileSwitchPopover from './MemberProfileSwitchPopover.vue'

const props = defineProps<NodeProps<ProfileNodeData>>()
const { t } = useI18n()
const router = useRouter()
const flowViewMode = inject<string>('flowViewMode', 'detail')
const openNodeDrawer = inject<(target: NodeDrawerTarget) => void>('openNodeDrawer')
const onConnectionChanged = inject<() => void>('onProfileSwitched', () => {})

const tempSourcePopoverRef = ref<InstanceType<typeof TempSourceSwitchPopover>>()
const functionPopoverRef = ref<InstanceType<typeof FunctionSwitchPopover>>()
const memberPopoverRef = ref<InstanceType<typeof MemberProfileSwitchPopover>>()

const showSwapButtons = computed(() => flowViewMode === 'detail' && !props.data.isDefault)

function onSwapTempSource(event: Event) {
    event.stopPropagation()
    tempSourcePopoverRef.value?.toggle(event)
}

function onSwapFunction(event: Event) {
    event.stopPropagation()
    functionPopoverRef.value?.toggle(event)
}

function onSwapMembers(event: Event) {
    event.stopPropagation()
    memberPopoverRef.value?.toggle(event)
}

function onClickProfile() {
    if (props.data.isDefault) return
    const target = {
        route: 'profiles',
        params: { profileUID: props.data.profileUID },
    }
    if (openNodeDrawer) {
        openNodeDrawer(target)
    } else {
        router.push({ name: target.route, params: target.params })
    }
}

function onClickFunction(e: Event) {
    e.stopPropagation()
    if (props.data.functionUID) {
        const target = {
            route: 'functions',
            params: { functionUID: props.data.functionUID },
        }
        if (openNodeDrawer) {
            openNodeDrawer(target)
        } else {
            router.push({ name: target.route, params: target.params })
        }
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
        class="group/node rounded-lg border border-border-one bg-bg-two shadow-md transition-shadow hover:shadow-lg"
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
            <!-- Swap button: temp source (Graph), members (Mix), base (Overlay) -->
            <div
                v-if="
                    showSwapButtons &&
                    (data.profileType === ProfileType.Graph ||
                        data.profileType === ProfileType.Mix ||
                        data.profileType === ProfileType.Overlay)
                "
                v-tooltip.top="
                    data.profileType === ProfileType.Graph
                        ? t('views.controls.switchTempSource')
                        : data.profileType === ProfileType.Mix
                          ? t('views.controls.switchMembers')
                          : t('views.controls.switchBaseProfile')
                "
                class="flex size-8 items-center justify-center rounded-md opacity-0 transition-all hover:bg-accent/15 group-hover/node:opacity-100"
                @click="
                    data.profileType === ProfileType.Graph
                        ? onSwapTempSource($event)
                        : onSwapMembers($event)
                "
            >
                <svg-icon
                    type="mdi"
                    :path="mdiSwapHorizontal"
                    class="size-5 text-text-color transition-colors hover:text-accent"
                />
            </div>
        </div>
        <div
            v-if="data.isDefault"
            class="px-3 pb-1 pt-0.5"
            v-tooltip.bottom="{ value: t('views.speed.defaultProfileInfo'), escape: false }"
        >
            <svg-icon
                type="mdi"
                class="size-4 text-warning"
                :path="mdiInformationSlabCircleOutline"
            />
        </div>
        <div class="space-y-1 px-3 pb-2">
            <div
                v-if="data.profileType === ProfileType.Fixed && data.speedFixed != null"
                class="text-xs text-text-color-secondary"
            >
                {{ t('models.profile.profileType.fixed') }}: {{ data.speedFixed
                }}{{ t('common.percentUnit') }}
            </div>
            <div
                v-if="data.profileType === ProfileType.Mix && data.mixFunctionType"
                class="text-xs text-text-color-secondary"
            >
                {{ t('models.profile.profileType.mix') }}: {{ data.mixFunctionType }}
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
                <span class="flex-1 truncate text-xs text-text-color-secondary">
                    {{ data.functionName }}
                </span>
                <span class="rounded bg-info/15 px-1 py-0.5 text-[10px] font-medium text-info">
                    {{ data.functionType }}
                </span>
                <div
                    v-if="showSwapButtons"
                    v-tooltip.top="t('views.controls.switchFunction')"
                    class="flex size-6 items-center justify-center rounded opacity-0 transition-all hover:bg-accent/15 group-hover/node:opacity-100"
                    @click="onSwapFunction"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiSwapHorizontal"
                        class="size-4 text-text-color-secondary transition-colors hover:text-accent"
                    />
                </div>
            </div>
        </div>
        <Handle type="source" :position="Position.Left" class="!bg-accent" />
        <Handle type="target" :position="Position.Right" class="!bg-accent" />

        <!-- Popovers -->
        <TempSourceSwitchPopover
            v-if="showSwapButtons && data.profileType === ProfileType.Graph"
            ref="tempSourcePopoverRef"
            :profile-u-i-d="data.profileUID"
            :current-device-u-i-d="data.tempSourceDeviceUID"
            :current-temp-name="data.tempSourceTempName"
            @changed="onConnectionChanged"
        />
        <FunctionSwitchPopover
            v-if="showSwapButtons && data.profileType === ProfileType.Graph"
            ref="functionPopoverRef"
            :profile-u-i-d="data.profileUID"
            :current-function-u-i-d="data.functionUID"
            @changed="onConnectionChanged"
        />
        <MemberProfileSwitchPopover
            v-if="
                showSwapButtons &&
                (data.profileType === ProfileType.Mix || data.profileType === ProfileType.Overlay)
            "
            ref="memberPopoverRef"
            :profile-u-i-d="data.profileUID"
            :profile-type="data.profileType"
            :current-member-u-i-ds="data.memberProfileUIDs"
            @changed="onConnectionChanged"
        />
    </div>
</template>
