// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { ChannelMetric } from '@/models/ChannelSource.ts'

// Above most fans. A new RPM alert and the fan fail alert start with this as "Greater Than".
export const RPM_LOCKED_MAX = 10_000
// A fan input can carry another unit, such as a pressure in microbar.
export const RPM_UNLOCKED_MAX = 9_999_999
// The daemon rejects equal thresholds.
export const THRESHOLD_GAP = 1

export const thresholdStep = (metric: ChannelMetric | undefined): number => {
    switch (metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return 1
        case ChannelMetric.RPM:
        case ChannelMetric.Freq:
            return 100
        case ChannelMetric.Temp:
        default:
            return 0.1
    }
}

// Only a metric whose values can leave the usual range gets a lock.
export const hasThresholdLock = (metric: ChannelMetric | undefined): boolean =>
    metric === ChannelMetric.RPM

export const thresholdMax = (metric: ChannelMetric | undefined, unlocked: boolean): number => {
    switch (metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return 100
        case ChannelMetric.RPM:
            return unlocked ? RPM_UNLOCKED_MAX : RPM_LOCKED_MAX
        case ChannelMetric.Freq:
            return 10_000
        case ChannelMetric.Temp:
        default:
            return 200
    }
}

// True when a saved threshold or a live reading does not fit the locked range.
export const needsUnlock = (
    metric: ChannelMetric | undefined,
    values: ReadonlyArray<number>,
): boolean => {
    if (!hasThresholdLock(metric)) return false
    const lockedMax = thresholdMax(metric, false)
    return values.some((value) => value > lockedMax)
}

// A slider offers min + n * step, so an unaligned min would offer 101, 201 and so on.
export const alignUpToStep = (value: number, step: number): number => {
    // The epsilon absorbs float noise such as 38.5 / 0.1 = 385.00000000000006.
    const steps = Math.ceil(value / step - 1e-6)
    return Number.parseFloat((steps * step).toFixed(10))
}
