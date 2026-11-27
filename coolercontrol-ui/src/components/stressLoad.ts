// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The live readout beside a running stress test. Pure, so it is tested without a store.

import { type Device, DeviceType, type UID } from '@/models/Device'
import type { ChannelValues } from '@/stores/DeviceStore.ts'

export interface LiveLoad {
    tempMax?: number
    watts?: number
}

// Kernel drivers for the temperature sensors on memory modules: DDR5 and DDR4.
const DIMM_DRIVERS = ['spd5118', 'jc42']

export const isCpu = (device: Device): boolean => device.type === DeviceType.CPU
export const isGpu = (device: Device): boolean => device.type === DeviceType.GPU
export const isDimm = (device: Device): boolean =>
    device.type === DeviceType.HWMON && DIMM_DRIVERS.includes(device.info?.driver_info.name ?? '')

// Hottest temperature and total power draw across the devices a given test
// targets. Utilization is useless for judging a stress test: it reads ~100%
// whether the hardware is near its power limit or nowhere close, so the
// numbers that tell you whether cooling is actually being exercised are
// temperature and watts.
export const liveLoad = (
    devices: Iterable<Device>,
    statuses: Map<UID, Map<string, ChannelValues>>,
    targets: (device: Device) => boolean,
): LiveLoad => {
    const load: LiveLoad = {}
    for (const device of devices) {
        if (!targets(device)) continue
        const channels = statuses.get(device.uid)
        if (channels == null) continue
        for (const values of channels.values()) {
            const temp = values.temp != null ? Number.parseFloat(values.temp) : Number.NaN
            if (!Number.isNaN(temp) && (load.tempMax === undefined || temp > load.tempMax)) {
                load.tempMax = temp
            }
            const power = values.watts != null ? Number.parseFloat(values.watts) : Number.NaN
            if (!Number.isNaN(power)) {
                load.watts = (load.watts ?? 0) + power
            }
        }
    }
    return load
}

export const formatTemp = (temp: number): string => `${temp.toFixed(0)}°C`

// The hottest reading seen so far. A missing reading leaves the peak where it was.
export const raisePeak = (
    peak: number | undefined,
    temp: number | undefined,
): number | undefined => {
    if (temp === undefined) return peak
    return peak === undefined ? temp : Math.max(peak, temp)
}

// The peak after a reading. A test going from idle to running starts a fresh peak, whoever
// started it. An idle test keeps the peak it ended with.
export const trackedPeak = (
    peak: number | undefined,
    temp: number | undefined,
    isActive: boolean,
    wasActive: boolean,
): number | undefined => {
    if (!isActive) return peak
    return raisePeak(wasActive ? peak : undefined, temp)
}

export interface LoadFormat {
    label?: string
}

// `71°C · 95W`, or with a label `CPU 71°C · 95W`. Empty when there is nothing to show.
export const formatLoad = (load: LiveLoad, { label }: LoadFormat = {}): string => {
    const parts: string[] = []
    if (load.tempMax !== undefined) parts.push(formatTemp(load.tempMax))
    if (load.watts !== undefined) parts.push(`${load.watts.toFixed(0)}W`)
    if (parts.length === 0) return ''
    const text = parts.join(' · ')
    return label ? `${label} ${text}` : text
}

// Translated by the caller, which keeps this module free of the i18n instance.
export interface RamLoadLabels {
    dimm: string
    cpu: string
}

// The RAM test's readout. The modules themselves come first where they have a sensor. The
// memory controller sits on the CPU package, so the CPU sensors reflect RAM stress as well.
export const formatRamLoad = (dimm: LiveLoad, cpu: LiveLoad, labels: RamLoadLabels): string => {
    if (dimm.tempMax === undefined) return formatLoad(cpu)
    return [
        formatLoad({ tempMax: dimm.tempMax }, { label: labels.dimm }),
        formatLoad(cpu, { label: labels.cpu }),
    ]
        .filter((part) => part.length > 0)
        .join(' · ')
}
