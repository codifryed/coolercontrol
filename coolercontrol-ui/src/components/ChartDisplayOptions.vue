<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiTuneVariant } from '@mdi/js'
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Dashboard } from '@/models/Dashboard.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import UiSwitch from '@/shell/ui/UiSwitch.vue'

interface Props {
    dashboard: Dashboard
}

defineProps<Props>()
const deviceStore = useDeviceStore()
const { t } = useI18n()
const isPopupOpen = ref(false)
</script>

<template>
    <div
        v-tooltip.bottom="{
            value: t('components.chartDisplayOptions.title'),
            disabled: isPopupOpen,
        }"
    >
        <popover-root @update:open="(open) => (isPopupOpen = open)">
            <popover-trigger
                class="h-10 rounded-lg border border-border-one bg-control !py-1.5 !px-4 text-text-color outline-0 text-center justify-center items-center flex !m-0 hover:bg-surface-hover"
                :aria-label="t('components.chartDisplayOptions.title')"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiTuneVariant"
                    :size="deviceStore.getREMSize(1.25)"
                />
            </popover-trigger>
            <popover-portal>
                <popover-content side="bottom" class="z-[1300]">
                    <div
                        class="flex min-w-56 flex-col gap-1 rounded-lg border border-border-one bg-bg-two p-2 text-text-color shadow-overlay-lg"
                    >
                        <div class="px-2 pb-1 font-bold">
                            {{ t('components.chartDisplayOptions.title') }}
                        </div>
                        <label
                            class="flex cursor-pointer items-center justify-between gap-6 rounded-md px-2 py-1.5 hover:bg-surface-hover"
                        >
                            <span>{{ t('components.chartDisplayOptions.statsLegend') }}</span>
                            <UiSwitch v-model="dashboard.showStatsLegend" />
                        </label>
                    </div>
                </popover-content>
            </popover-portal>
        </popover-root>
    </div>
</template>
