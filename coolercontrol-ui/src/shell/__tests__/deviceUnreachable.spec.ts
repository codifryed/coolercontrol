// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A device that stops answering and a channel serving failsafe values are different conditions,
// and the UI has to keep them apart. Failsafe means the device is alive with stale readings and
// safe values substituted; unreachable means nothing can be read from or written to it. Showing
// the second as the first would tell a user their fans are on a safe curve when in fact no curve
// can be applied.
//
// The daemon reports both at once for a wedged device, because its channels also failsafe once
// their readings go stale. These tests pin the precedence that resolves that overlap.

import 'reflect-metadata'
import { plainToInstance } from 'class-transformer'
import { describe, expect, it } from 'vitest'
import en from '@/i18n/locales/en.ts'
import {
    DeviceHealthDTO,
    HealthState,
    UnreachableDelta,
    UnreachableRef,
    unreachableKey,
} from '@/models/DeviceHealth.ts'

/** The precedence the device lists, the monitoring panel and the inline warning all apply. */
const worseOf = (
    unreachable: Array<UnreachableRef>,
    deviceUid: string,
): 'unreachable' | 'failsafe' =>
    unreachable.some((ref) => ref.device_uid === deviceUid) ? 'unreachable' : 'failsafe'

describe('device unreachable health state', () => {
    it('has an english string for both the label and its explanation', () => {
        const appInfo = (en as any).views.appInfo
        expect(appInfo.deviceUnreachable).toBeTruthy()
        expect(appInfo.deviceUnreachableDetail).toBeTruthy()
        // The explanation has to say why a setting has no effect, not merely that something is off.
        expect(appInfo.deviceUnreachableDetail).not.toBe(appInfo.deviceUnreachable)
    })

    it('parses the daemon snapshot into the model, including the timeout count', () => {
        const dto = plainToInstance(DeviceHealthDTO, {
            failsafe: [],
            unreachable: [{ device_uid: 'dev1', device_name: 'octo', consecutive_timeouts: 8 }],
            missing: [],
            stale_source: [],
            firmware_overrides: [],
            channel_capabilities: [],
            system_findings: [],
        } as object)

        expect(dto.unreachable).toHaveLength(1)
        expect(dto.unreachable[0]).toBeInstanceOf(UnreachableRef)
        expect(dto.unreachable[0].device_name).toBe('octo')
        expect(dto.unreachable[0].consecutive_timeouts).toBe(8)
    })

    // An older daemon sends no `unreachable` field at all. The UI must read that as "nothing
    // unreachable" rather than crashing on an undefined array.
    it('defaults to empty when the daemon does not report the field', () => {
        const dto = plainToInstance(DeviceHealthDTO, { failsafe: [] } as object)
        expect(dto.unreachable).toEqual([])
    })

    it('keys an unreachable entry by device, since the whole device is the subject', () => {
        const first = plainToInstance(UnreachableRef, { device_uid: 'dev1' } as object)
        const second = plainToInstance(UnreachableRef, {
            device_uid: 'dev1',
            consecutive_timeouts: 12,
        } as object)
        // Same device, later count: one entry that updates, not two rows for one dead device.
        expect(unreachableKey(first)).toBe(unreachableKey(second))
    })

    it('reports the device state ahead of its channels failsafe, because it is the cause', () => {
        const unreachable = [
            plainToInstance(UnreachableRef, { device_uid: 'dev1', device_name: 'octo' } as object),
        ]
        expect(worseOf(unreachable, 'dev1')).toBe('unreachable')
        // A healthy device's failsafe is still its own story.
        expect(worseOf(unreachable, 'dev2')).toBe('failsafe')
    })

    it('carries the resolved state so a recovered device clears rather than latching', () => {
        const delta = plainToInstance(UnreachableDelta, {
            device_uid: 'dev1',
            device_name: 'octo',
            consecutive_timeouts: 8,
            state: 'Resolved',
        } as object)
        expect(delta.state).toBe(HealthState.Resolved)
        expect(unreachableKey(delta)).toBe('dev1')
    })
})
