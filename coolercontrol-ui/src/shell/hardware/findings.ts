// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Pure selection logic for the machine-scope hardware notices. Kept free of
// i18n so it can be tested directly; the strings live in useHardwareText.
import { SystemFindingKind, type SystemFinding } from '@/models/DeviceHealth.ts'

/**
 * Findings worth putting in front of a user. `DetectionUnsupported` is a
 * statement about the build, not a problem with this machine, so it would put a
 * docs link in front of every non-x86_64 user whose fans work. The daemon
 * leaves it out of its actionable set for the same reason.
 */
export function actionableFindings(findings: Array<SystemFinding>): Array<SystemFinding> {
    return findings.filter((finding) => finding.kind !== SystemFindingKind.DetectionUnsupported)
}

/** Which headline the cooling notice leads with, or none when all is well. */
export type HardwareNoticeKind =
    'signatureRejected' | 'blockedByEnvironment' | 'noChannels' | 'noneControllable'

/**
 * A notice is raised only when the channels themselves are the problem: a
 * machine whose fans all work has nothing to be told, even if a probe was
 * blocked on the way there. When one is raised, a cause wins the headline
 * because it explains the channel count rather than restating it. A rejected
 * signature names the driver that went missing, so it beats a blocked probe.
 */
export function hardwareNoticeKind(
    findings: Array<SystemFinding>,
    channelCount: number,
    controllableCount: number,
): HardwareNoticeKind | undefined {
    if (channelCount > 0 && controllableCount > 0) return undefined
    const actionable = actionableFindings(findings)
    const hasKind = (kind: SystemFindingKind) => actionable.some((finding) => finding.kind === kind)
    if (hasKind(SystemFindingKind.SignatureRejected)) return 'signatureRejected'
    if (hasKind(SystemFindingKind.BlockedByEnvironment)) return 'blockedByEnvironment'
    return channelCount === 0 ? 'noChannels' : 'noneControllable'
}
