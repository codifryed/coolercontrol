<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// One reference value in the alert editor. Selecting it offers to use it as a threshold.
import { computed } from 'vue'
import { DropdownMenuItem } from 'reka-ui'
import { useI18n } from 'vue-i18n'
import UiDropdownMenu from '@/shell/ui/UiDropdownMenu.vue'
import { dropdownItemClass } from '@/shell/ui/dropdownItemClass.ts'
import { groupDigits } from '@/shell/digitGroups.ts'
import {
    applyBlock,
    type ApplyBlock,
    type ThresholdTarget,
    type Thresholds,
} from '@/components/alertReference.ts'

const props = withDefaults(
    defineProps<{
        // The number as shown. Exactly this is applied, with no headroom added.
        text: string
        thresholds: Thresholds
        label?: string
        // Shown on hover, e.g. the sysfs file a driver limit comes from.
        hint?: string
    }>(),
    { label: '', hint: '' },
)
const emit = defineEmits<{ apply: [target: ThresholdTarget, value: number] }>()
const { t } = useI18n()

const value = computed(() => Number(props.text))
const BLOCK_KEYS: Record<ApplyBlock, string> = {
    crossesOther: 'views.alerts.crossesOtherThreshold',
    outsideRange: 'views.alerts.outsideRange',
}
const targets = computed(() =>
    (['max', 'min'] as const).map((target) => ({
        target,
        label: t(target === 'max' ? 'views.alerts.useAsGreaterThan' : 'views.alerts.useAsLessThan'),
        block: applyBlock(target, value.value, props.thresholds),
    })),
)
</script>

<template>
    <UiDropdownMenu>
        <template #trigger>
            <button
                type="button"
                v-tooltip.top="{ value: hint, disabled: hint === '' }"
                class="-mx-1 whitespace-nowrap rounded px-1 outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent data-[state=open]:bg-surface-hover"
            >
                <span v-if="label" class="text-text-color-secondary">{{ label }}&nbsp;</span
                >{{ groupDigits(text) }}
            </button>
        </template>
        <DropdownMenuItem
            v-for="item in targets"
            :key="item.target"
            :disabled="item.block != null"
            :class="[dropdownItemClass, 'data-[disabled]:cursor-default']"
            @select="emit('apply', item.target, value)"
        >
            <span class="flex flex-col">
                <span :class="{ 'opacity-50': item.block != null }">{{ item.label }}</span>
                <span v-if="item.block != null" class="text-sm text-text-color-secondary">
                    {{ t(BLOCK_KEYS[item.block]) }}
                </span>
            </span>
        </DropdownMenuItem>
    </UiDropdownMenu>
</template>
