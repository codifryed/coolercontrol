// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import {
    mdiFan,
    mdiGauge,
    mdiLightningBoltCircle,
    mdiSineWave,
    mdiSpeedometer,
    mdiThermometer,
} from '@mdi/js'
import type { Device } from '@/models/Device.ts'
import type { ChannelValues } from '@/stores/DeviceStore.ts'
import { isFanChannel, type ChannelUnitOf } from '@/shell/channelUnit.ts'

export type ChannelKind = 'temp' | 'fan' | 'load' | 'freq' | 'power' | 'sensor'

// Kind comes from device metadata (temps, speed_options) and, for the remaining
// sensors, the reported value field. Failsafed/stale sensors keep reporting
// values, so field-based classification stays stable without flicker. A speed value
// that is not a fan's is a sensor in its own unit, a flow or a pressure.
export function channelKind(
    device: Device | undefined,
    channelName: string,
    values: ChannelValues | undefined,
    unitOf: ChannelUnitOf,
): ChannelKind {
    if (device?.info?.temps.has(channelName)) return 'temp'
    if (isFanChannel(device, channelName, unitOf)) return 'fan'
    if (values?.freq != null) return 'freq'
    if (values?.watts != null) return 'power'
    if (values?.rpm != null && values.duty == null) return 'sensor'
    return 'load'
}

const KIND_ICONS: Record<ChannelKind, string> = {
    temp: mdiThermometer,
    fan: mdiFan,
    load: mdiSpeedometer,
    sensor: mdiGauge,
    freq: mdiSineWave,
    power: mdiLightningBoltCircle,
}

export function channelKindIcon(kind: ChannelKind): string {
    return KIND_ICONS[kind]
}

// Fans spin under Eye Candy when actually moving (rpm when reported, else duty).
export function channelSpins(
    kind: ChannelKind,
    values: ChannelValues | undefined,
    eyeCandy: boolean,
): boolean {
    if (!eyeCandy || kind !== 'fan') return false
    return values?.rpm != null ? Number(values.rpm) > 0 : Number(values?.duty ?? 0) > 0
}
