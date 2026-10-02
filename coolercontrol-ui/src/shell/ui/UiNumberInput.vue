<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiMinus, mdiPlus } from '@mdi/js'
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'
import { groupDigits, ungroupDigits } from '@/shell/digitGroups.ts'

const model = defineModel<number>({ required: true })
const props = withDefaults(
    defineProps<{
        min?: number
        max?: number
        // Recommended band inside min/max. Values outside it still apply, they
        // just render in the warning color. Defaults to min/max, i.e. no band.
        safeMin?: number
        safeMax?: number
        step?: number
        prefix?: string
        suffix?: string
        disabled?: boolean
        // Groups a long number in threes while the field is not being edited. A number
        // field cannot show the separators, so this renders a text field.
        grouped?: boolean
    }>(),
    {
        min: Number.MIN_SAFE_INTEGER,
        max: Number.MAX_SAFE_INTEGER,
        safeMin: undefined,
        safeMax: undefined,
        step: 1,
        prefix: '',
        suffix: '',
        disabled: false,
        grouped: false,
    },
)

const clamp = (value: number): number => Math.min(props.max, Math.max(props.min, value))
// Strip binary float noise (0.1 + 0.2 style) from step arithmetic.
const round = (value: number): number => Number.parseFloat(value.toFixed(10))
const stepBy = (direction: number): void => {
    model.value = round(clamp((model.value ?? 0) + direction * props.step))
}

// Wheel-over-input adjusts the value, as the raw inputs this component replaced did. It
// lives here rather than in each view: the views attached their listeners by querying
// class and id hooks that only existed on those raw inputs, so every one of them silently
// became a no-op. Scrolling up raises the value, matching the increment button above.
const onWheel = (event: WheelEvent): void => {
    if (props.disabled || event.deltaY === 0) return
    event.preventDefault()
    stepBy(event.deltaY < 0 ? 1 : -1)
}

// Press-and-hold repeats the step: one step on press, then auto-repeat.
let holdDelay: ReturnType<typeof setTimeout> | undefined
let holdRepeat: ReturnType<typeof setInterval> | undefined
const stopHold = (): void => {
    clearTimeout(holdDelay)
    clearInterval(holdRepeat)
    window.removeEventListener('pointerup', stopHold)
}
const startHold = (direction: number): void => {
    stopHold()
    stepBy(direction)
    window.addEventListener('pointerup', stopHold)
    holdDelay = setTimeout(() => {
        holdRepeat = setInterval(() => stepBy(direction), 75)
    }, 400)
}
onBeforeUnmount(stopHold)

// Keyboard activation arrives as a click with detail 0; pointer presses are
// handled by startHold and must not double-step through click.
const keyboardStep = (event: MouseEvent, direction: number): void => {
    if (event.detail === 0) stepBy(direction)
}
const onInput = (event: Event): void => {
    const input = event.target as HTMLInputElement
    const value = Number(ungroupDigits(input.value))
    if (!Number.isNaN(value)) model.value = clamp(value)
    // Clamping back to the value already held changes nothing reactive, and neither does
    // rejected text, so either would stay in the field while the model holds another value.
    void nextTick(() => {
        input.value = shownValue.value
    })
}
// A number field steps on the arrow keys by itself; the text field needs it done.
const onKeydown = (event: KeyboardEvent): void => {
    if (!props.grouped) return
    if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return
    event.preventDefault()
    stepBy(event.key === 'ArrowUp' ? 1 : -1)
}
// Fractional steps keep a fixed precision: 10.5 + 0.5 reads 11.0, not 11.
// A finer value typed by hand keeps its own precision.
const decimalsOf = (value: number | undefined): number => {
    const text = String(value ?? '')
    const dot = text.indexOf('.')
    return dot === -1 ? 0 : text.length - dot - 1
}
const displayValue = computed(() => {
    if (model.value == null) return ''
    const digits = Math.max(decimalsOf(props.step), decimalsOf(model.value))
    return digits > 0 ? model.value.toFixed(digits) : String(model.value)
})
const editing = ref(false)
const shownValue = computed(() =>
    props.grouped && !editing.value ? groupDigits(displayValue.value) : displayValue.value,
)
// Entering the field swaps the grouped text for plain digits. The browser places the caret
// after the focus event, so the swap waits a task and then puts the caret back on its digit.
const onFocus = (event: FocusEvent): void => {
    if (!props.grouped) return
    const input = event.target as HTMLInputElement
    setTimeout(() => {
        if (document.activeElement !== input) return
        const text = input.value
        const digitsBefore = (caret: number | null): number =>
            ungroupDigits(text.slice(0, caret ?? text.length)).length
        const start = digitsBefore(input.selectionStart)
        const end = digitsBefore(input.selectionEnd)
        editing.value = true
        void nextTick(() => input.setSelectionRange(start, end))
    })
}
// Size the input to its content so the suffix hugs the number.
const inputWidth = computed(() => `${Math.max(shownValue.value.length, 1) + 1}ch`)
const outsideSafeBand = computed(() => {
    if (model.value == null) return false
    if (props.safeMin != null && model.value < props.safeMin) return true
    return props.safeMax != null && model.value > props.safeMax
})
</script>

<template>
    <span
        class="inline-flex h-10 items-stretch overflow-hidden rounded-lg border bg-control"
        :class="[
            outsideSafeBand ? 'border-warning' : 'border-border-one',
            { 'pointer-events-none opacity-50': disabled },
        ]"
        @wheel="onWheel"
    >
        <button
            type="button"
            class="px-2 text-text-color-secondary outline-none hover:bg-surface-hover hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
            :disabled="disabled"
            @pointerdown.prevent="startHold(-1)"
            @pointerleave="stopHold"
            @click="keyboardStep($event, -1)"
        >
            <svg-icon type="mdi" :path="mdiMinus" :size="14" />
        </button>
        <span class="flex min-w-16 items-center justify-center px-1">
            <span v-if="prefix" class="pr-0.5 text-sm text-text-color-secondary">
                {{ prefix }}
            </span>
            <input
                :type="grouped ? 'text' : 'number'"
                :inputmode="grouped ? 'decimal' : undefined"
                :value="shownValue"
                :min="min"
                :max="max"
                :step="step"
                :disabled="disabled"
                class="bg-transparent text-right text-base outline-none focus-visible:ring-2 focus-visible:ring-accent"
                :class="outsideSafeBand ? 'text-warning' : 'text-text-color'"
                :style="{ width: inputWidth }"
                @change="onInput"
                @focus="onFocus"
                @blur="editing = false"
                @keydown="onKeydown"
            />
            <span v-if="suffix" class="pl-0.5 text-sm text-text-color-secondary">
                {{ suffix }}
            </span>
        </span>
        <button
            type="button"
            class="px-2 text-text-color-secondary outline-none hover:bg-surface-hover hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
            :disabled="disabled"
            @pointerdown.prevent="startHold(1)"
            @pointerleave="stopHold"
            @click="keyboardStep($event, 1)"
        >
            <svg-icon type="mdi" :path="mdiPlus" :size="14" />
        </button>
    </span>
</template>

<style scoped>
input::-webkit-outer-spin-button,
input::-webkit-inner-spin-button {
    -webkit-appearance: none;
    margin: 0;
}
input[type='number'] {
    -moz-appearance: textfield;
    appearance: textfield;
}
</style>
