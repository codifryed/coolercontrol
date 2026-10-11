<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { ToggleGroupItem, ToggleGroupRoot } from 'reka-ui'
import { computed } from 'vue'

export interface UiToggleOption {
    label: string
    value: string
    disabled?: boolean
}

type Size = 'sm' | 'md'

const model = defineModel<string>({ required: true })
withDefaults(defineProps<{ options: UiToggleOption[]; size?: Size }>(), { size: 'md' })

// Single-select that cannot be toggled off to an empty state.
const groupModel = computed<string>({
    get: () => model.value,
    set: (value) => {
        if (value != null && value !== '') model.value = value
    },
})

const rootSizes: Record<Size, string> = { sm: 'h-7', md: 'h-10' }
const itemSizes: Record<Size, string> = { sm: 'px-2 py-0.5 text-sm', md: 'px-3 py-1 text-base' }
</script>

<template>
    <ToggleGroupRoot
        v-model="groupModel"
        type="single"
        class="inline-flex items-center rounded-lg border border-border-one bg-control p-0.5"
        :class="rootSizes[size]"
    >
        <ToggleGroupItem
            v-for="option in options"
            :key="option.value"
            :value="option.value"
            :disabled="option.disabled"
            class="cursor-pointer whitespace-nowrap rounded-md text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent data-[state=on]:bg-accent data-[state=on]:text-accent-fg data-[disabled]:pointer-events-none data-[disabled]:opacity-50"
            :class="itemSizes[size]"
        >
            {{ option.label }}
        </ToggleGroupItem>
    </ToggleGroupRoot>
</template>
