// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import { DIGIT_GROUP_SEPARATOR as S } from '@/shell/digitGroups.ts'

interface InputProps {
    modelValue: number
    min?: number
    max?: number
    step?: number
    grouped?: boolean
}

const mounted: Array<{ unmount: () => void }> = []
function mountInput(props: InputProps) {
    const wrapper = mount(UiNumberInput, {
        props: {
            ...props,
            'onUpdate:modelValue': (value: number) => wrapper.setProps({ modelValue: value }),
        },
        global: { stubs: { SvgIcon: true } },
        // Focus only moves to an element that is in the document.
        attachTo: document.body,
    })
    mounted.push(wrapper)
    return wrapper
}

const type = async (wrapper: ReturnType<typeof mountInput>, text: string): Promise<void> => {
    const input = wrapper.get('input')
    input.element.value = text
    await input.trigger('change')
    await nextTick()
}

// The swap to plain digits waits a task after the focus event.
const focus = async (wrapper: ReturnType<typeof mountInput>): Promise<void> => {
    vi.useFakeTimers()
    wrapper.get('input').element.focus()
    vi.runAllTimers()
    vi.useRealTimers()
    await nextTick()
}

afterEach(() => {
    vi.useRealTimers()
    mounted.splice(0).forEach((wrapper) => wrapper.unmount())
})

describe('UiNumberInput', () => {
    it('shows the bound when a typed value is clamped to the value already held', async () => {
        // The alert editor's rpm ceiling: typing a pressure reading kept showing the typed
        // number while the alert saved the ceiling.
        const wrapper = mountInput({ modelValue: 10_000, max: 10_000 })
        await type(wrapper, '438300')
        expect(wrapper.props('modelValue')).toBe(10_000)
        expect(wrapper.get('input').element.value).toBe('10000')
    })

    it('keeps a typed value inside the range', async () => {
        const wrapper = mountInput({ modelValue: 10_000, min: 0, max: 9_999_999 })
        await type(wrapper, '438300')
        expect(wrapper.props('modelValue')).toBe(438_300)
        expect(wrapper.get('input').element.value).toBe('438300')
    })

    it('accepts a value that is not a multiple of the step', async () => {
        const wrapper = mountInput({ modelValue: 0, min: 0, max: 10_000, step: 100 })
        await type(wrapper, '59')
        expect(wrapper.props('modelValue')).toBe(59)
    })

    it('stays a number field unless grouping is asked for', () => {
        const wrapper = mountInput({ modelValue: 438_300 })
        expect(wrapper.get('input').attributes('type')).toBe('number')
        expect(wrapper.get('input').element.value).toBe('438300')
    })
})

describe('UiNumberInput grouped', () => {
    it('groups a long number while the field is not being edited', () => {
        const wrapper = mountInput({ modelValue: 438_300, grouped: true })
        expect(wrapper.get('input').attributes('type')).toBe('text')
        expect(wrapper.get('input').element.value).toBe(`438${S}300`)
    })

    it('leaves a short number as it is', () => {
        const wrapper = mountInput({ modelValue: 1500, grouped: true })
        expect(wrapper.get('input').element.value).toBe('1500')
    })

    it('shows plain digits while editing and groups again on leaving', async () => {
        const wrapper = mountInput({ modelValue: 438_300, grouped: true })
        const input = wrapper.get('input')
        await focus(wrapper)
        expect(input.element.value).toBe('438300')
        input.element.blur()
        await nextTick()
        expect(input.element.value).toBe(`438${S}300`)
    })

    it('keeps the caret on its digit when the separators go', async () => {
        const wrapper = mountInput({ modelValue: 9_999_999, grouped: true })
        const input = wrapper.get('input').element
        vi.useFakeTimers()
        input.focus()
        // "9 999 999": the caret sits after the fourth digit, past one separator.
        input.setSelectionRange(5, 5)
        vi.runAllTimers()
        vi.useRealTimers()
        await nextTick()
        await nextTick()
        expect(input.value).toBe('9999999')
        expect(input.selectionStart).toBe(4)
    })

    it('takes a typed or pasted number with separators', async () => {
        const wrapper = mountInput({ modelValue: 0, min: 0, max: 9_999_999, grouped: true })
        await type(wrapper, '438 300')
        expect(wrapper.props('modelValue')).toBe(438_300)
        await type(wrapper, `382${S}800`)
        expect(wrapper.props('modelValue')).toBe(382_800)
        expect(wrapper.get('input').element.value).toBe(`382${S}800`)
    })

    it('puts the held value back when the text is not a number', async () => {
        const wrapper = mountInput({ modelValue: 438_300, grouped: true })
        await type(wrapper, 'abc')
        expect(wrapper.props('modelValue')).toBe(438_300)
        expect(wrapper.get('input').element.value).toBe(`438${S}300`)
    })

    it('steps on the arrow keys', async () => {
        const wrapper = mountInput({
            modelValue: 500,
            min: 0,
            max: 10_000,
            step: 100,
            grouped: true,
        })
        await wrapper.get('input').trigger('keydown', { key: 'ArrowUp' })
        expect(wrapper.props('modelValue')).toBe(600)
        await wrapper.get('input').trigger('keydown', { key: 'ArrowDown' })
        await wrapper.get('input').trigger('keydown', { key: 'ArrowDown' })
        expect(wrapper.props('modelValue')).toBe(400)
    })
})
