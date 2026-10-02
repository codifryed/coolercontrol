// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A channel is shown in several places, each with its own row template. Every
// one of them has to mark an unhealthy channel and say which condition it is:
// failsafe values in use, or a device that stopped responding.

import 'reflect-metadata'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { reactive, ref } from 'vue'
import { createI18n } from 'vue-i18n'
import { createMemoryHistory, createRouter } from 'vue-router'
import { mdiAlert, mdiLanDisconnect } from '@mdi/js'
import en from '@/i18n/locales/en.ts'

const DEVICE = 'dev1'
const CUSTOM_SENSORS = 'cs1'
const DASHBOARD = 'dash1'

const CHANNELS = {
    fan: { deviceUID: DEVICE, channelName: 'fan1' },
    temp: { deviceUID: DEVICE, channelName: 'temp1' },
    'custom sensor': { deviceUID: CUSTOM_SENSORS, channelName: 'sensor1' },
}
type ChannelKind = keyof typeof CHANNELS

const devices = [
    {
        uid: DEVICE,
        type: 'Hwmon',
        info: {
            temps: new Map([['temp1', {}]]),
            channels: new Map([
                [
                    'fan1',
                    {
                        speed_options: { min_duty: 0, max_duty: 100 },
                        lighting_modes: [],
                        lcd_modes: [],
                    },
                ],
            ]),
        },
    },
    {
        uid: CUSTOM_SENSORS,
        type: 'CustomSensors',
        info: { temps: new Map([['sensor1', {}]]), channels: new Map() },
    },
]

const settings = reactive({
    pinnedIds: [] as string[],
    healthFailsafe: [] as Array<{ device_uid: string; name: string; reason?: string }>,
    healthUnreachable: [] as Array<{ device_uid: string }>,
    healthMissing: [],
    healthStaleSource: [],
    allUIDeviceSettings: new Map(),
    allDaemonDeviceSettings: new Map(),
    ccDeviceSettings: new Map(),
    dashboards: [{ uid: DASHBOARD, name: 'Overview' }],
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
        allDevices: () => devices,
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

// A marker is its tooltip text and its icon: the two conditions differ in both.
const FAILSAFE = [`${en.views.appInfo.failsafeActive}: stale readings`, mdiAlert]
const UNREACHABLE = [
    `${en.views.appInfo.deviceUnreachable}: ${en.views.appInfo.deviceUnreachableDetail}`,
    mdiLanDisconnect,
]

const failsafe = ({ deviceUID, channelName }: { deviceUID: string; channelName: string }) => ({
    device_uid: deviceUID,
    name: channelName,
    reason: 'stale readings',
})

const mountOptions = {
    global: {
        plugins: [i18n, router],
        stubs: {
            SvgIcon: true,
            RouterLink: { template: '<a><slot /></a>' },
            // The real tooltip only renders its text on hover.
            UiTooltip: { props: ['text'], template: '<span :data-tooltip="text"><slot /></span>' },
            PanelHeader: true,
            TagChips: true,
            TagPopover: true,
            CCColorPicker: true,
            AlertSilenceMenu: true,
            LibraryList: true,
            HardwareHelpLine: true,
            CalibrationBadge: true,
            FirmwareCurveBadge: true,
            UncontrollableBadge: true,
            ChannelMiniGraph: true,
            ChannelSetupMenu: true,
        },
        directives: { tooltip: {} },
    },
}

const panels = {
    Home: () => import('@/shell/home/HomePanel.vue'),
    Monitoring: () => import('@/shell/monitoring/MonitoringPanel.vue'),
    Cooling: () => import('@/shell/cooling/CoolingPanel.vue'),
}
type PanelName = keyof typeof panels

// Cooling lists fans only.
const pinnable: Array<[PanelName, ChannelKind]> = [
    ['Home', 'fan'],
    ['Home', 'temp'],
    ['Home', 'custom sensor'],
    ['Monitoring', 'fan'],
    ['Monitoring', 'temp'],
    ['Monitoring', 'custom sensor'],
    ['Cooling', 'fan'],
]

const mountPanel = async (name: PanelName) => {
    const { default: Panel } = await panels[name]()
    return mount(Panel, mountOptions)
}

const markers = (wrapper: ReturnType<typeof mount>, within = '') =>
    wrapper
        .findAll(`${within} [data-tooltip]`)
        .map((marker) => [
            marker.attributes('data-tooltip'),
            marker.get('svg-icon-stub').attributes('path'),
        ])

const pinnedMarkers = (wrapper: ReturnType<typeof mount>) => markers(wrapper, '[data-panel-pinned]')

beforeEach(() => {
    settings.pinnedIds = []
    settings.healthFailsafe = []
    settings.healthUnreachable = []
})

describe.each(pinnable)('%s panel, pinned %s', (name, kind) => {
    const channel = CHANNELS[kind]

    beforeEach(() => {
        settings.pinnedIds = [`${channel.deviceUID}_${channel.channelName}`]
    })

    it('says failsafe values are in use', async () => {
        settings.healthFailsafe = [failsafe(channel)]
        expect(pinnedMarkers(await mountPanel(name))).toEqual([FAILSAFE])
    })

    // The daemon reports a wedged device's channels as failsafe too.
    it('says the device stopped responding, not failsafe', async () => {
        settings.healthUnreachable = [{ device_uid: channel.deviceUID }]
        settings.healthFailsafe = [failsafe(channel)]
        expect(pinnedMarkers(await mountPanel(name))).toEqual([UNREACHABLE])
    })

    it('marks a device that stopped responding before any channel failsafes', async () => {
        settings.healthUnreachable = [{ device_uid: channel.deviceUID }]
        expect(pinnedMarkers(await mountPanel(name))).toEqual([UNREACHABLE])
    })

    it('leaves a healthy channel unmarked', async () => {
        settings.healthFailsafe = [failsafe({ deviceUID: channel.deviceUID, channelName: 'other' })]
        const wrapper = await mountPanel(name)
        expect(wrapper.find('[data-panel-pinned]').exists()).toBe(true)
        expect(pinnedMarkers(wrapper)).toEqual([])
    })
})

describe.each(['Home', 'Monitoring'] as const)('%s panel, pinned dashboard', (name) => {
    it('carries no channel health', async () => {
        settings.pinnedIds = [DASHBOARD]
        settings.healthUnreachable = [{ device_uid: DEVICE }]
        settings.healthFailsafe = [failsafe(CHANNELS.fan)]
        const wrapper = await mountPanel(name)
        expect(wrapper.find('[data-panel-pinned]').exists()).toBe(true)
        expect(pinnedMarkers(wrapper)).toEqual([])
    })
})

describe('Cooling page fan card', () => {
    const mountCard = async () => {
        const { default: ChannelCard } = await import('@/shell/cooling/ChannelCard.vue')
        return mount(ChannelCard, {
            ...mountOptions,
            props: {
                channel: { ...CHANNELS.fan, controllable: true, minDuty: 0, maxDuty: 100 },
            },
        })
    }

    it('says failsafe values are in use', async () => {
        settings.healthFailsafe = [failsafe(CHANNELS.fan)]
        expect(markers(await mountCard())).toEqual([FAILSAFE])
    })

    it('says the device stopped responding, not failsafe', async () => {
        settings.healthUnreachable = [{ device_uid: DEVICE }]
        settings.healthFailsafe = [failsafe(CHANNELS.fan)]
        expect(markers(await mountCard())).toEqual([UNREACHABLE])
    })

    it('marks a device that stopped responding before any channel failsafes', async () => {
        settings.healthUnreachable = [{ device_uid: DEVICE }]
        expect(markers(await mountCard())).toEqual([UNREACHABLE])
    })

    it('leaves a healthy fan unmarked', async () => {
        settings.healthFailsafe = [failsafe(CHANNELS.temp)]
        expect(markers(await mountCard())).toEqual([])
    })
})

describe('Devices panel', () => {
    const mountDevices = async () => {
        const { default: DevicesPanel } = await import('@/shell/devices/DevicesPanel.vue')
        return mount(DevicesPanel, mountOptions)
    }

    it('says failsafe values are in use on a custom sensor row', async () => {
        settings.healthFailsafe = [failsafe(CHANNELS['custom sensor'])]
        expect(markers(await mountDevices())).toEqual([FAILSAFE])
    })

    it('says failsafe values are in use on a device row', async () => {
        settings.healthFailsafe = [failsafe(CHANNELS.fan)]
        expect(markers(await mountDevices())).toEqual([FAILSAFE])
    })

    it('says a device stopped responding, not failsafe', async () => {
        settings.healthUnreachable = [{ device_uid: DEVICE }]
        settings.healthFailsafe = [failsafe(CHANNELS.fan)]
        expect(markers(await mountDevices())).toEqual([UNREACHABLE])
    })
})
