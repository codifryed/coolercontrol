// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { type Device, DeviceType, type UID } from '@/models/Device.ts'

export interface MonitoringSensor {
    deviceUID: UID
    channelName: string
    isTemp: boolean
}

export interface MonitoringDeviceGroup {
    deviceUID: UID
    sensors: MonitoringSensor[]
}

// The Monitoring sensor tree: temps plus all value-bearing channels including
// fans (duty/rpm are monitored too); lighting/LCD belong to Devices. The
// CustomSensors device is monitored like any other; editing lives in Devices.
// It stays listed without sensors, since its header is where new ones are added.
export function monitoringSensors(devices: Iterable<Device>): MonitoringDeviceGroup[] {
    const groups: MonitoringDeviceGroup[] = []
    for (const device of devices) {
        if (device.info == null) continue
        const sensors: MonitoringSensor[] = []
        for (const tempName of device.info.temps.keys()) {
            sensors.push({ deviceUID: device.uid, channelName: tempName, isTemp: true })
        }
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lighting_modes.length > 0 || channelInfo.lcd_info != null) {
                continue
            }
            sensors.push({ deviceUID: device.uid, channelName, isTemp: false })
        }
        if (sensors.length > 0 || device.type === DeviceType.CUSTOM_SENSORS) {
            groups.push({ deviceUID: device.uid, sensors })
        }
    }
    return groups
}
