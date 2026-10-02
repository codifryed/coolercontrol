<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import type { UID } from '@/models/Device.ts'
import { useDeviceHealth } from '@/composables/useDeviceHealth.ts'
import UiTooltip from '@/shell/ui/UiTooltip.vue'

defineProps<{
    deviceUID: UID
    channelName: string
}>()

const { isUnhealthy, healthIcon, healthTooltip } = useDeviceHealth()
</script>

<template>
    <UiTooltip
        v-if="isUnhealthy(deviceUID, channelName)"
        :text="healthTooltip(deviceUID, channelName)"
    >
        <svg-icon type="mdi" :path="healthIcon(deviceUID)" :size="14" class="shrink-0 text-error" />
    </UiTooltip>
</template>
