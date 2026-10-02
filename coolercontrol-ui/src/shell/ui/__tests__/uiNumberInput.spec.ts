// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'

function mountInput(props: { modelValue: number; min?: number; max?: number }) {
    const wrapper = mount(UiNumberInput, {
        props: {
            ...props,
            'onUpdate:modelValue': (value: number) => wrapper.setProps({ modelValue: value }),
        },
        global: { stubs: { SvgIcon: true } },
    })
    return wrapper
}

const type = async (wrapper: ReturnType<typeof mountInput>, text: string): Promise<void> => {
    const input = wrapper.get('input')
    input.element.value = text
    await input.trigger('change')
    await nextTick()
}

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
        const wrapper = mount(UiNumberInput, {
            props: { modelValue: 0, min: 0, max: 10_000, step: 100 },
            global: { stubs: { SvgIcon: true } },
        })
        const input = wrapper.get('input')
        input.element.value = '59'
        await input.trigger('change')
        expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([59])
    })
})
