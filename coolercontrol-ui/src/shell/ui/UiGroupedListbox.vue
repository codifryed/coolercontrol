<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiCheck, mdiMagnify, mdiMemory, mdiMinusThick } from '@mdi/js'
import { computed, nextTick, onMounted, ref } from 'vue'

export interface UiGroupedOption {
    label: string
    value: string
    /** Sensor line color rendered as a dash before the label. */
    color?: string
    /** Render the color as a round dot (tags) instead of a line (sensors). */
    dot?: boolean
    /** Right-aligned live value, e.g. '45.0 °C'. */
    rightText?: string
    disabled?: boolean
}

export interface UiOptionGroup {
    label: string
    options: UiGroupedOption[]
    /** Keys the group. Needed only when two groups share a label, as
     *  unlabelled runs between labelled groups do. */
    id?: string
    /** Heading icon; the memory chip when a group does not name one. */
    icon?: string
    /** Heading icon color, such as the device's color; the text color when unset. */
    color?: string
}

// Grouped single- or multi-select option list with optional filtering.
const model = defineModel<string | string[] | undefined>()
const props = withDefaults(
    defineProps<{
        groups: UiOptionGroup[]
        filter?: boolean
        filterPlaceholder?: string
        multiple?: boolean
        invalid?: boolean
        emptyText?: string
    }>(),
    { filter: false, filterPlaceholder: '', multiple: false, invalid: false, emptyText: '' },
)

const filterText = ref('')
const visibleGroups = computed(() => {
    const needle = filterText.value.trim().toLowerCase()
    if (!needle) return props.groups
    return props.groups
        .map((group) => ({
            ...group,
            options: group.options.filter((option) => option.label.toLowerCase().includes(needle)),
        }))
        .filter((group) => group.options.length > 0)
})

const isSelected = (value: string): boolean =>
    props.multiple ? ((model.value as string[]) ?? []).includes(value) : model.value === value

const toggle = (value: string): void => {
    if (props.multiple) {
        const current = [...(((model.value as string[]) ?? []) as string[])]
        const index = current.indexOf(value)
        if (index >= 0) {
            current.splice(index, 1)
        } else {
            current.push(value)
        }
        model.value = current
    } else {
        model.value = value
    }
}

const scrollContainer = ref<HTMLDivElement>()

// Center the first selected option on mount: these lists can be long and the
// current selection may sit far below the fold. Manual scrollTop math keeps
// ancestor scroll containers untouched (scrollIntoView would move them too).
onMounted(async () => {
    await nextTick()
    const container = scrollContainer.value
    if (container == null) return
    const selected = container.querySelector<HTMLElement>('[data-selected]')
    if (selected == null) return
    const containerRect = container.getBoundingClientRect()
    const selectedRect = selected.getBoundingClientRect()
    container.scrollTop +=
        selectedRect.top -
        containerRect.top -
        (container.clientHeight / 2 - selected.clientHeight / 2)
})
</script>

<template>
    <div
        class="flex min-h-0 flex-col rounded-lg border bg-control"
        :class="invalid ? 'border-error' : 'border-border-one'"
    >
        <div v-if="filter" class="flex items-center gap-2 border-b border-border-one px-3 py-2">
            <svg-icon
                type="mdi"
                :path="mdiMagnify"
                :size="16"
                class="shrink-0 text-text-color-secondary"
            />
            <input
                v-model="filterText"
                type="text"
                :placeholder="filterPlaceholder"
                class="w-full bg-transparent text-base text-text-color outline-none"
            />
        </div>
        <div ref="scrollContainer" class="min-h-0 flex-1 overflow-y-auto p-1">
            <template v-for="group in visibleGroups" :key="group.id ?? group.label">
                <div
                    v-if="group.label !== ''"
                    class="flex items-center gap-2 px-2 py-1.5 text-sm font-semibold text-text-color"
                >
                    <slot name="group" :group="group">
                        <svg-icon
                            type="mdi"
                            :path="group.icon ?? mdiMemory"
                            :size="18"
                            class="shrink-0"
                            :style="group.color ? { color: group.color } : undefined"
                        />
                        {{ group.label }}
                    </slot>
                </div>
                <button
                    v-for="option in group.options"
                    :key="option.value"
                    type="button"
                    :data-selected="isSelected(option.value) ? 'true' : undefined"
                    :disabled="option.disabled"
                    class="flex w-full items-center gap-2 rounded-md py-1.5 pr-2 text-base text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent disabled:pointer-events-none disabled:opacity-50"
                    :class="[
                        group.label === '' ? 'pl-2' : 'pl-6',
                        { 'bg-surface-hover': isSelected(option.value) },
                    ]"
                    @click="toggle(option.value)"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiCheck"
                        :size="14"
                        class="shrink-0"
                        :class="isSelected(option.value) ? 'text-accent' : 'invisible'"
                    />
                    <span
                        v-if="option.color && option.dot"
                        class="h-3 w-3 shrink-0 rounded-full"
                        :style="{ backgroundColor: option.color }"
                    />
                    <svg-icon
                        v-else-if="option.color"
                        type="mdi"
                        :path="mdiMinusThick"
                        :size="14"
                        class="shrink-0"
                        :style="{ color: option.color }"
                    />
                    <span class="truncate">{{ option.label }}</span>
                    <span
                        v-if="option.rightText"
                        class="ml-auto shrink-0 pl-2 text-text-color-secondary"
                    >
                        {{ option.rightText }}
                    </span>
                </button>
            </template>
            <div
                v-if="visibleGroups.length === 0"
                class="px-3 py-4 text-center text-sm text-text-color-secondary"
            >
                {{ emptyText }}
            </div>
        </div>
    </div>
</template>
