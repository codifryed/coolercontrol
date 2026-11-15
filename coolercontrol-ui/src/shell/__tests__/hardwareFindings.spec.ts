// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// DeviceHealth's class-transformer decorators need the polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import { SystemFinding, SystemFindingKind, EnvironmentBlocker } from '@/models/DeviceHealth.ts'
import { actionableFindings, hardwareNoticeKind } from '@/shell/hardware/findings.ts'

function finding(kind: SystemFindingKind, reason?: EnvironmentBlocker): SystemFinding {
    const value = new SystemFinding()
    value.kind = kind
    value.reason = reason
    return value
}

describe('actionableFindings', () => {
    it('drops DetectionUnsupported, which is about the build not the machine', () => {
        const findings = [
            finding(SystemFindingKind.DetectionUnsupported),
            finding(SystemFindingKind.NoDriverBound),
        ]
        expect(actionableFindings(findings).map((f) => f.kind)).toEqual([
            SystemFindingKind.NoDriverBound,
        ])
    })

    it('keeps every other kind', () => {
        const findings = [
            finding(SystemFindingKind.NoDriverBound),
            finding(SystemFindingKind.Blacklisted),
            finding(SystemFindingKind.BlockedByEnvironment, EnvironmentBlocker.SecureBoot),
        ]
        expect(actionableFindings(findings)).toHaveLength(3)
    })
})

describe('hardwareNoticeKind', () => {
    it('says nothing when channels exist and at least one is controllable', () => {
        expect(hardwareNoticeKind([], 4, 1)).toBeUndefined()
    })

    it('reports no channels when none were detected', () => {
        expect(hardwareNoticeKind([], 0, 0)).toBe('noChannels')
    })

    it('reports none controllable when channels exist but none can be driven', () => {
        expect(hardwareNoticeKind([], 4, 0)).toBe('noneControllable')
    })

    // The blocked probe explains the channel count rather than restating it,
    // so it has to win over both count-based cases.
    it('leads with a blocked probe over the channel count', () => {
        const blocked = [
            finding(SystemFindingKind.BlockedByEnvironment, EnvironmentBlocker.Container),
        ]
        expect(hardwareNoticeKind(blocked, 0, 0)).toBe('blockedByEnvironment')
        expect(hardwareNoticeKind(blocked, 4, 0)).toBe('blockedByEnvironment')
    })

    // Every channel working is not a reason to raise a blocked probe: the
    // machine plainly did not need it.
    it('stays silent when a probe was blocked but the fans work anyway', () => {
        const blocked = [
            finding(SystemFindingKind.BlockedByEnvironment, EnvironmentBlocker.SecureBoot),
        ]
        expect(hardwareNoticeKind(blocked, 4, 4)).toBeUndefined()
    })

    // A rejected signature names its driver, and the notice lists it with its
    // own cause, so the headline stays on the channel count.
    it('leaves the headline to the channel count for a rejected signature', () => {
        const findings = [finding(SystemFindingKind.SignatureRejected)]
        expect(hardwareNoticeKind(findings, 0, 0)).toBe('noChannels')
        expect(hardwareNoticeKind(findings, 4, 0)).toBe('noneControllable')
    })

    it('stays silent about a rejected signature when the fans work anyway', () => {
        const findings = [finding(SystemFindingKind.SignatureRejected)]
        expect(hardwareNoticeKind(findings, 4, 2)).toBeUndefined()
    })

    it('ignores DetectionUnsupported when picking the headline', () => {
        const findings = [finding(SystemFindingKind.DetectionUnsupported)]
        expect(hardwareNoticeKind(findings, 0, 0)).toBe('noChannels')
    })
})
