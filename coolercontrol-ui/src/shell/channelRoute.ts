// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { RouteLocationRaw } from 'vue-router'
import type { ChannelInfo } from '@/models/ChannelInfo.ts'
import { type Device, DeviceType, type UID } from '@/models/Device.ts'

// Canonical page for a channel/sensor, entity-first: the same target no matter
// which section links to it. Custom sensors edit under Devices; fan/pump
// channels (speed_options) live on their Cooling page, which embeds the same
// chart; everything else is a read-only sensor chart under Monitoring.
export function channelRoute(
    devices: Iterable<Device>,
    deviceUID: UID,
    channelName: string,
): RouteLocationRaw {
    for (const device of devices) {
        if (device.uid !== deviceUID) continue
        if (device.type === DeviceType.CUSTOM_SENSORS) {
            return { name: 'device-custom-sensor', params: { customSensorID: channelName } }
        }
        if (device.info?.channels.get(channelName)?.speed_options != null) {
            return { name: 'cooling-channel', params: { deviceUID, channelName } }
        }
        break
    }
    return { name: 'monitoring-sensor', params: { deviceUID, channelName } }
}

// Target for a channel listed by the Monitoring section itself. Fans and custom
// sensors stay on their full chart here instead of being thrown over to Cooling
// or Devices, which keeps the section and its panel in step; DashboardView
// carries the companion link to their control page. Everything else keeps the
// canonical target. Shortcut surfaces (pinned rows, the Home panel) stay on
// `channelRoute`.
export function monitoringChannelRoute(
    devices: Iterable<Device>,
    deviceUID: UID,
    channelName: string,
): RouteLocationRaw {
    for (const device of devices) {
        if (device.uid !== deviceUID) continue
        if (
            device.type === DeviceType.CUSTOM_SENSORS ||
            device.info?.channels.get(channelName)?.speed_options != null
        ) {
            return { name: 'monitoring-sensor', params: { deviceUID, channelName } }
        }
        break
    }
    return channelRoute(devices, deviceUID, channelName)
}

// Target for a controllable channel listed by its capability rather than by a
// device lookup, as the Mode table lists them. Lighting and LCD channels are
// controllable but have no sensor chart, so the canonical route would send them
// to an empty Monitoring page; they get their own editor instead.
export function controlChannelRoute(
    channelInfo: ChannelInfo,
    deviceUID: UID,
    channelName: string,
): RouteLocationRaw {
    const params = { deviceUID, channelName }
    if (channelInfo.speed_options != null) return { name: 'cooling-channel', params }
    if (channelInfo.lighting_modes.length > 0) return { name: 'device-lighting', params }
    if (channelInfo.lcd_info != null) return { name: 'device-lcd', params }
    return { name: 'monitoring-sensor', params }
}
