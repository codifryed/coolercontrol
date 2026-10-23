// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import UiGroupedListbox, { type UiOptionGroup } from '@/shell/ui/UiGroupedListbox.vue'

const option = { label: 'Temp 1', value: 'temp1' }

function headingIconColors(groups: UiOptionGroup[]): Array<string> {
    const wrapper = mount(UiGroupedListbox, {
        props: { groups, modelValue: undefined },
        global: { stubs: { SvgIcon: true } },
    })
    // Only headings render an 18px icon: options use 14px ones.
    return wrapper
        .findAll('svg-icon-stub[size="18"]')
        .map((icon) => (icon.element as HTMLElement).style.color)
}

describe('UiGroupedListbox group headings', () => {
    it('tints the heading icon with the group color', () => {
        const colors = headingIconColors([{ label: 'CPU', color: '#ff0000', options: [option] }])
        expect(colors).toEqual(['rgb(255, 0, 0)'])
    })

    it('leaves the heading icon on the text color without a group color', () => {
        const colors = headingIconColors([{ label: 'GPU', options: [option] }])
        expect(colors).toEqual([''])
    })
})
