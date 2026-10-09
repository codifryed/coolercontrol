// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { type Device, DeviceType, type UID } from '@/models/Device.ts'
import type { MenuOrderIds } from '@/models/UISettings.ts'
import { isUnitSensor, type ChannelUnitOf } from '@/shell/channelUnit.ts'
import { orderedByGroup } from '@/shell/panelOrder.ts'

// Canonical device-type ordering for the Devices section (panel and landing).
export const DEVICE_TYPE_ORDER: DeviceType[] = [
    DeviceType.CPU,
    DeviceType.GPU,
    DeviceType.HWMON,
    DeviceType.LIQUIDCTL,
    DeviceType.SERVICE_PLUGIN,
    DeviceType.CUSTOM_SENSORS,
]

export function deviceTypeRank(type: DeviceType): number {
    const index = DEVICE_TYPE_ORDER.indexOf(type)
    return index >= 0 ? index : DEVICE_TYPE_ORDER.length
}

export interface DeviceTypeGroup {
    type: DeviceType
    devices: Device[]
}

export function deviceTypeGroups(devices: Device[]): DeviceTypeGroup[] {
    return DEVICE_TYPE_ORDER.map((type) => ({
        type,
        devices: devices.filter((device) => device.type === type),
    })).filter((group) => group.devices.length > 0)
}

// The Devices section lists all devices including the virtual CustomSensors
// device; its sensors are created and edited here.
export function hardwareDevices(devices: Iterable<Device>): Device[] {
    return [...devices]
}

// Sensor names of a CustomSensors device (editor child links). A temperature sensor is
// a temp of the device, a sensor of any other metric one of its channels. The saved
// order is applied across both, as the device store only sorts each map on its own.
export function customSensorNames(device: Device, menuOrder: MenuOrderIds[]): string[] {
    if (device.type !== DeviceType.CUSTOM_SENSORS || device.info == null) return []
    const names = [...device.info.temps.keys(), ...device.info.channels.keys()]
    return orderedByGroup(menuOrder, device.uid, names, (name) => `${device.uid}_${name}`)
}

export interface DeviceChannelLink {
    deviceUID: UID
    channelName: string
    kind: 'lighting' | 'lcd'
}

// Lighting and LCD channels get their own editor subpages.
export function deviceChannelLinks(device: Device): DeviceChannelLink[] {
    const links: DeviceChannelLink[] = []
    if (device.info == null) return links
    for (const [channelName, channelInfo] of device.info.channels.entries()) {
        if (channelInfo.lighting_modes.length > 0) {
            links.push({ deviceUID: device.uid, channelName, kind: 'lighting' })
        }
    }
    for (const [channelName, channelInfo] of device.info.channels.entries()) {
        if (channelInfo.lcd_modes.length > 0) {
            links.push({ deviceUID: device.uid, channelName, kind: 'lcd' })
        }
    }
    return links
}

// Where a device channel/sensor is viewed or controlled. Read-only sensors go
// to Monitoring; controllable channels to Cooling; lighting and LCD to their
// own editors.
// Enumerable at runtime on purpose: each kind names a `layout.shell.sensorDest.*`
// translation key that only ever appears as a template literal, which an unused-key
// sweep cannot see. The i18n spec walks this array so a prune cannot silently drop them.
export const SENSOR_LINK_KINDS = ['monitoring', 'cooling', 'lighting', 'lcd'] as const

export type SensorLinkKind = (typeof SENSOR_LINK_KINDS)[number]

export interface DeviceSensorLink {
    channelName: string
    kind: SensorLinkKind
}

// Every live channel/sensor of a device with the page it links to, so the
// device page can present one complete list (temps and read channels, fans and
// pumps, lighting, LCD). Mirrors sensorToggles' enumeration order.
export function deviceSensorLinks(device: Device, unitOf: ChannelUnitOf): DeviceSensorLink[] {
    const links: DeviceSensorLink[] = []
    const seen = new Set<string>()
    const add = (channelName: string, kind: SensorLinkKind): void => {
        if (seen.has(channelName)) return
        seen.add(channelName)
        links.push({ channelName, kind })
    }
    for (const temp of device.status.temps) {
        add(temp.name, 'monitoring')
    }
    for (const keyword of ['freq', 'power', 'load']) {
        for (const channel of device.status.channels) {
            if (channel.name.toLowerCase().includes(keyword)) add(channel.name, 'monitoring')
        }
    }
    if (device.info != null) {
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            const unitSensor = isUnitSensor(device, channelName, unitOf)
            if (channelInfo.speed_options != null) {
                add(channelName, unitSensor ? 'monitoring' : 'cooling')
                continue
            }
            // A read-only value in another unit, such as a flow meter. Lighting and LCD
            // channels have no reading, whatever their label ends in.
            const readable =
                channelInfo.lighting_modes.length === 0 && channelInfo.lcd_modes.length === 0
            if (unitSensor && readable) add(channelName, 'monitoring')
        }
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lighting_modes.length > 0) add(channelName, 'lighting')
        }
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lcd_modes.length > 0) add(channelName, 'lcd')
        }
    }
    return links
}

export interface SensorToggle {
    channelName: string
    enabled: boolean
}

// Every toggleable sensor/channel of a device, mirroring the old settings
// tree: temps, freq/power/load channels, fan channels, lighting, LCD, plus
// currently-disabled channels persisted in the CC device settings.
export function sensorToggles(
    device: Device,
    disabledChannelNames: Iterable<string>,
): SensorToggle[] {
    const toggles: SensorToggle[] = []
    const seen = new Set<string>()
    const add = (channelName: string, enabled: boolean): void => {
        if (seen.has(channelName)) return
        seen.add(channelName)
        toggles.push({ channelName, enabled })
    }
    for (const temp of device.status.temps) {
        add(temp.name, true)
    }
    for (const keyword of ['freq', 'power', 'load']) {
        for (const channel of device.status.channels) {
            if (channel.name.toLowerCase().includes(keyword)) add(channel.name, true)
        }
    }
    if (device.info != null) {
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.speed_options != null) add(channelName, true)
        }
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lighting_modes.length > 0) add(channelName, true)
        }
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lcd_modes.length > 0) add(channelName, true)
        }
    }
    const disabled = [...disabledChannelNames].filter((name) => !seen.has(name))
    disabled.sort((a, b) => a.localeCompare(b))
    for (const channelName of disabled) {
        add(channelName, false)
    }
    return toggles
}
