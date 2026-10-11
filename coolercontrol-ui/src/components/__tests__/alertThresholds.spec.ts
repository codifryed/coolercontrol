// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import {
    alignUpToStep,
    hasThresholdLock,
    needsUnlock,
    RPM_LOCKED_MAX,
    RPM_UNLOCKED_MAX,
    thresholdMax,
    thresholdStep,
} from '@/components/alertThresholds.ts'
import { ChannelMetric } from '@/models/ChannelSource.ts'

describe('thresholdMax', () => {
    it('only widens the rpm range when unlocked', () => {
        expect(thresholdMax(ChannelMetric.RPM, false)).toBe(RPM_LOCKED_MAX)
        expect(thresholdMax(ChannelMetric.RPM, true)).toBe(RPM_UNLOCKED_MAX)
        for (const metric of [ChannelMetric.Temp, ChannelMetric.Duty, ChannelMetric.Load]) {
            expect(thresholdMax(metric, true)).toBe(thresholdMax(metric, false))
        }
    })

    it('fits the largest known fan input value when unlocked', () => {
        // The Leakshield scales a u16 by 100 for its pressure in microbar.
        expect(RPM_UNLOCKED_MAX).toBeGreaterThanOrEqual(65_535 * 100)
    })

    it('falls back to the temperature range without a metric', () => {
        expect(thresholdMax(undefined, false)).toBe(thresholdMax(ChannelMetric.Temp, false))
        expect(thresholdStep(undefined)).toBe(thresholdStep(ChannelMetric.Temp))
    })
})

describe('needsUnlock', () => {
    it('is true for a value above the locked rpm range', () => {
        expect(needsUnlock(ChannelMetric.RPM, [1200, 438_300])).toBe(true)
        // An alert saved under the previous 30000 ceiling.
        expect(needsUnlock(ChannelMetric.RPM, [1, 30_000])).toBe(true)
    })

    it('is false at or below the locked rpm range', () => {
        expect(needsUnlock(ChannelMetric.RPM, [0, RPM_LOCKED_MAX])).toBe(false)
        expect(needsUnlock(ChannelMetric.RPM, [])).toBe(false)
    })

    it('is false for a metric without a lock', () => {
        expect(hasThresholdLock(ChannelMetric.Temp)).toBe(false)
        expect(needsUnlock(ChannelMetric.Temp, [500])).toBe(false)
        expect(needsUnlock(undefined, [438_300])).toBe(false)
    })
})

describe('alignUpToStep', () => {
    it('raises an unaligned value to the next step', () => {
        expect(alignUpToStep(1, 100)).toBe(100)
        expect(alignUpToStep(60, 100)).toBe(100)
        expect(alignUpToStep(101, 100)).toBe(200)
    })

    it('keeps an aligned value', () => {
        expect(alignUpToStep(0, 100)).toBe(0)
        expect(alignUpToStep(300, 100)).toBe(300)
        expect(alignUpToStep(51, 1)).toBe(51)
    })

    it('ignores float noise on fractional steps', () => {
        expect(alignUpToStep(38.5, 0.1)).toBe(38.5)
        expect(alignUpToStep(1, 0.1)).toBe(1)
        expect(alignUpToStep(38.55, 0.1)).toBe(38.6)
    })
})
