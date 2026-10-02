// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A pinned row is a second copy of a channel's row, with its own template. The
// failsafe marker was added to the listed rows and missed on two of the three
// pinned ones, so a pinned fan could sit on failsafe values with nothing on the
// row saying so.

import 'reflect-metadata'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { reactive, ref } from 'vue'
import { createI18n } from 'vue-i18n'
import { createMemoryHistory, createRouter } from 'vue-router'
import { mdiAlert } from '@mdi/js'
import en from '@/i18n/locales/en.ts'

const DEVICE = 'dev1'
const FAN = 'fan1'
const TEMP = 'temp1'

const device = {
    uid: DEVICE,
    type: 'Hwmon',
    info: {
        temps: new Map([[TEMP, {}]]),
        channels: new Map([
            [FAN, { speed_options: { min_duty: 0, max_duty: 100 }, lighting_modes: [] }],
        ]),
    },
}

const settings = reactive({
    pinnedIds: [] as string[],
    healthFailsafe: [] as Array<{ device_uid: string; name: string; reason?: string }>,
    healthUnreachable: [] as Array<{ device_uid: string }>,
    healthMissing: [],
    healthStaleSource: [],
    allUIDeviceSettings: new Map(),
    dashboards: [],
    alerts: [],
    alertsNeedingAttention: [],
    profiles: [],
    functions: [],
    menuOrder: [],
    homeDashboard: undefined,
    eyeCandy: false,
    frequencyPrecision: 1,
})

vi.mock('@/stores/SettingsStore.ts', () => ({ useSettingsStore: () => settings }))

vi.mock('@/stores/DeviceStore.ts', () => ({
    DEFAULT_NAME_STRING_LENGTH: 40,
    useDeviceStore: () => ({
        allDevices: () => [device],
        currentDeviceStatus: ref(new Map()),
        reSortDevicesByMenuOrder: () => {},
    }),
}))

vi.mock('@/stores/ThemeColorsStore.ts', () => ({
    useThemeColorsStore: () => ({ themeColors: { text_color: '0 0 0' } }),
}))

vi.mock('@/shell/routeActive.ts', () => ({ useRouteActive: () => () => false }))

vi.mock('@/composables/useLibraryWizards.ts', () => ({
    useLibraryWizards: () => ({ openProfileWizard: () => {}, openFunctionWizard: () => {} }),
}))

const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } })
const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { render: () => null } }],
})

const panels = {
    Home: () => import('@/shell/home/HomePanel.vue'),
    Monitoring: () => import('@/shell/monitoring/MonitoringPanel.vue'),
    Cooling: () => import('@/shell/cooling/CoolingPanel.vue'),
}

const mountPanel = async (name: keyof typeof panels) => {
    const { default: Panel } = await panels[name]()
    return mount(Panel, {
        global: {
            plugins: [i18n, router],
            stubs: {
                SvgIcon: true,
                RouterLink: { template: '<a><slot /></a>' },
                PanelHeader: true,
                TagChips: true,
                TagPopover: true,
                CCColorPicker: true,
                AlertSilenceMenu: true,
                LibraryList: true,
                HardwareHelpLine: true,
            },
            directives: { tooltip: {} },
        },
    })
}

const pinnedMarkers = (wrapper: Awaited<ReturnType<typeof mountPanel>>) =>
    wrapper.findAll(`[data-panel-pinned] svg-icon-stub[path="${mdiAlert}"]`)

beforeEach(() => {
    settings.pinnedIds = [`${DEVICE}_${FAN}`]
    settings.healthFailsafe = []
    settings.healthUnreachable = []
})

describe.each(Object.keys(panels) as Array<keyof typeof panels>)('%s panel pinned rows', (name) => {
    it('marks a channel that is on failsafe values', async () => {
        settings.healthFailsafe = [{ device_uid: DEVICE, name: FAN, reason: 'stale readings' }]
        const wrapper = await mountPanel(name)
        expect(pinnedMarkers(wrapper)).toHaveLength(1)
    })

    // Unreachable is reported ahead of the channels going stale, so the row
    // cannot wait for a failsafe entry to say something is wrong.
    it('marks a channel whose device stopped responding', async () => {
        settings.healthUnreachable = [{ device_uid: DEVICE }]
        const wrapper = await mountPanel(name)
        expect(pinnedMarkers(wrapper)).toHaveLength(1)
    })

    it('leaves a healthy channel unmarked', async () => {
        settings.healthFailsafe = [{ device_uid: DEVICE, name: TEMP }]
        const wrapper = await mountPanel(name)
        expect(wrapper.find('[data-panel-pinned]').exists()).toBe(true)
        expect(pinnedMarkers(wrapper)).toHaveLength(0)
    })
})
