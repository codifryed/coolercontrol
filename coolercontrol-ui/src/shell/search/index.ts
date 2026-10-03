// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Builds the palette's index from the lists the sections already derive, so a
// channel is classified here exactly as the Cooling and Monitoring panels
// classify it, and lands on the same page `channelRoute` would send it to.
//
// A plain function taking its inputs, never a `computed`. The device status
// stream ticks every poll interval; a reactive index would rebuild on each one
// for no gain, since the palette is closed the rest of the time. The caller
// builds on open.

import { type Device, DeviceType, type UID } from '@/models/Device.ts'
import { channelRoute } from '@/shell/channelRoute.ts'
import type { ChannelUnitOf } from '@/shell/channelUnit.ts'
import { coolingChannels } from '@/shell/cooling/channels.ts'
import { deviceChannelLinks } from '@/shell/devices/devices.ts'
import { monitoringSensors } from '@/shell/monitoring/sensors.ts'
import { ACTION_ENTRIES } from '@/shell/search/actionCatalog.ts'
import { PAGE_ENTRIES } from '@/shell/search/pages.ts'
import { SETTINGS_ENTRIES } from '@/shell/search/settingsCatalog.ts'
import type { SearchEntry } from '@/shell/search/types.ts'

interface Named {
    uid: UID
    name: string
}

export interface IndexDeps {
    devices: readonly Device[]
    /** Display name for a device, user renames included. */
    deviceLabel: (deviceUID: UID) => string
    /** Display name for a channel or sensor, user renames included. */
    channelLabel: (deviceUID: UID, channelName: string) => string
    /** The unit a channel's label names, which makes an uncontrollable fan input a sensor. */
    channelUnit: ChannelUnitOf
    profiles: readonly Named[]
    functions: readonly Named[]
    modes: readonly Named[]
    dashboards: readonly Named[]
    alerts: readonly Named[]
    pluginIds: readonly string[]
    isQtApp: boolean
    /** Active-locale lookup. */
    t: (key: string) => string
    /** English lookup, indexed alongside `t` so docs terminology still finds things. */
    tEn: (key: string) => string
}

const COOLING = 'layout.shell.cooling'
const MONITORING = 'layout.shell.monitoring'
const DEVICES = 'layout.shell.devices'
const SETTINGS = 'layout.shell.settings'
const PLUGINS = 'layout.shell.plugins'

export function buildIndex(deps: IndexDeps): SearchEntry[] {
    const entries: SearchEntry[] = []
    const { t, tEn, deviceLabel, channelLabel } = deps

    // Section landings, sub-pages and the settings cards.
    for (const page of PAGE_ENTRIES) {
        if (page.qtOnly === true && !deps.isQtApp) continue
        if (page.pluginsOnly === true && deps.pluginIds.length === 0) continue
        entries.push({
            id: page.id,
            kind: 'page',
            label: t(page.labelKey),
            labelEn: tEn(page.labelKey),
            keywords: page.keywords,
            breadcrumb: [t(page.groupKey)],
            target: {
                route: {
                    name: page.routeName,
                    ...(page.params != null && { params: page.params }),
                },
            },
        })
    }

    for (const entry of SETTINGS_ENTRIES) {
        if (entry.qtOnly === true && !deps.isQtApp) continue
        const crumbs = [t(SETTINGS), t(entry.cardKey)]
        if (entry.groupKey != null) crumbs.push(t(entry.groupKey))
        entries.push({
            id: entry.id,
            kind: 'setting',
            label: t(entry.labelKey),
            labelEn: tEn(entry.labelKey),
            keywords: entry.keywords,
            breadcrumb: crumbs,
            // The anchor doubles as the route param: AppSettings resolves an
            // unknown param straight through getElementById.
            target: { route: { name: 'settings', params: { tabNumber: entry.id } } },
        })
    }

    for (const action of ACTION_ENTRIES) {
        if (action.qtOnly === true && !deps.isQtApp) continue
        entries.push({
            id: `action-${action.id}`,
            kind: 'action',
            label: t(action.labelKey),
            labelEn: tEn(action.labelKey),
            keywords: action.keywords,
            breadcrumb: action.breadcrumbKeys.map((key) => t(key)),
            target: { action: action.id },
        })
    }

    for (const device of deps.devices) {
        entries.push({
            id: `device-${device.uid}`,
            kind: 'device',
            label: deviceLabel(device.uid),
            breadcrumb: [t(DEVICES)],
            target: { route: { name: 'devices-device', params: { deviceUID: device.uid } } },
        })
    }

    for (const group of coolingChannels(deps.devices, deps.channelUnit)) {
        for (const channel of group.channels) {
            entries.push({
                id: `fan-${group.deviceUID}-${channel.channelName}`,
                kind: 'fan',
                label: channelLabel(group.deviceUID, channel.channelName),
                breadcrumb: [t(COOLING), deviceLabel(group.deviceUID)],
                target: {
                    route: {
                        name: 'cooling-channel',
                        params: {
                            deviceUID: group.deviceUID,
                            channelName: channel.channelName,
                        },
                    },
                },
            })
        }
    }

    // A fan appears under Cooling above and is not repeated here; custom sensors
    // get their own kind, since they are edited rather than only watched.
    const coolingIds = new Set(
        coolingChannels(deps.devices, deps.channelUnit).flatMap((group) =>
            group.channels.map((channel) => `${group.deviceUID}-${channel.channelName}`),
        ),
    )
    const customSensorDevices = new Set(
        deps.devices.filter((d) => d.type === DeviceType.CUSTOM_SENSORS).map((d) => d.uid),
    )
    for (const group of monitoringSensors(deps.devices)) {
        for (const sensor of group.sensors) {
            if (coolingIds.has(`${group.deviceUID}-${sensor.channelName}`)) continue
            const custom = customSensorDevices.has(group.deviceUID)
            entries.push({
                id: `${custom ? 'custom' : 'sensor'}-${group.deviceUID}-${sensor.channelName}`,
                kind: custom ? 'customSensor' : 'sensor',
                label: channelLabel(group.deviceUID, sensor.channelName),
                breadcrumb: [
                    t(custom ? DEVICES : MONITORING),
                    custom ? t('layout.menu.customSensors') : deviceLabel(group.deviceUID),
                ],
                target: {
                    route: channelRoute(
                        deps.devices,
                        group.deviceUID,
                        sensor.channelName,
                        deps.channelUnit,
                    ),
                },
            })
        }
    }

    for (const device of deps.devices) {
        for (const link of deviceChannelLinks(device)) {
            entries.push({
                id: `${link.kind}-${device.uid}-${link.channelName}`,
                kind: link.kind,
                label: channelLabel(device.uid, link.channelName),
                breadcrumb: [t(DEVICES), deviceLabel(device.uid)],
                target: {
                    route: {
                        name: link.kind === 'lighting' ? 'device-lighting' : 'device-lcd',
                        params: { deviceUID: device.uid, channelName: link.channelName },
                    },
                },
            })
        }
    }

    const library = [
        {
            items: deps.profiles,
            kind: 'profile',
            route: 'profiles',
            param: 'profileUID',
            crumbs: [COOLING, 'layout.shell.coolingPanel.profiles'],
        },
        {
            items: deps.functions,
            kind: 'function',
            route: 'functions',
            param: 'functionUID',
            crumbs: [COOLING, 'layout.shell.coolingPanel.functions'],
        },
        {
            items: deps.modes,
            kind: 'mode',
            route: 'modes',
            param: 'modeUID',
            crumbs: [COOLING, 'layout.shell.modes'],
        },
        {
            items: deps.dashboards,
            kind: 'dashboard',
            route: 'monitoring-dashboard',
            param: 'dashboardUID',
            crumbs: [MONITORING, 'layout.menu.dashboards'],
        },
        {
            items: deps.alerts,
            kind: 'alert',
            route: 'monitoring-alert',
            param: 'alertUID',
            crumbs: [MONITORING, 'layout.menu.alerts'],
        },
    ] as const

    for (const set of library) {
        for (const item of set.items) {
            entries.push({
                id: `${set.kind}-${item.uid}`,
                kind: set.kind,
                label: item.name,
                breadcrumb: set.crumbs.map((key) => t(key)),
                target: { route: { name: set.route, params: { [set.param]: item.uid } } },
            })
        }
    }

    for (const pluginId of deps.pluginIds) {
        entries.push({
            id: `plugin-${pluginId}`,
            kind: 'plugin',
            label: pluginId,
            breadcrumb: [t(PLUGINS)],
            target: { route: { name: 'plugin-page', params: { pluginId } } },
        })
    }

    return entries
}
