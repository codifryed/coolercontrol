<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// Header button to a channel's or sensor's page under Monitoring.
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiChartLine } from '@mdi/js'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import type { UID } from '@/models/Device.ts'
import UiButton from '@/shell/ui/UiButton.vue'

const props = defineProps<{
    deviceUID: UID
    channelName: string
}>()

const { t } = useI18n()
const router = useRouter()

const openFullChart = (): void => {
    router.push({
        name: 'monitoring-sensor',
        params: { deviceUID: props.deviceUID, channelName: props.channelName },
    })
}
</script>

<template>
    <UiButton
        variant="outline"
        v-tooltip.top="t('layout.shell.coolingPage.fullChart')"
        @click="openFullChart"
    >
        <svg-icon type="mdi" :path="mdiChartLine" :size="16" />
        {{ t('layout.shell.monitoring') }}
    </UiButton>
</template>
