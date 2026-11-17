// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One definition of the machine-scope hardware strings, shared by the Home
// support card and the cooling notices so the two cannot drift apart.
import { useI18n } from 'vue-i18n'
import { EnvironmentBlocker, SystemFindingKind, type SystemFinding } from '@/models/DeviceHealth.ts'
import type { HardwareNoticeKind } from '@/shell/hardware/findings.ts'

export function useHardwareText() {
    const { t } = useI18n()

    // Keys are written out per case rather than interpolated. A template
    // literal key is invisible to the static i18n sweep, and an unused-key
    // prune has silently deleted that kind of key before.
    function blockerDetail(reason: EnvironmentBlocker | undefined): string {
        switch (reason) {
            case EnvironmentBlocker.SecureBoot:
                return t('views.appInfo.findingBlockedBySecureBoot')
            case EnvironmentBlocker.Container:
                return t('views.appInfo.findingBlockedByContainer')
            case EnvironmentBlocker.NoDevPort:
                return t('views.appInfo.findingBlockedByNoDevPort')
            default:
                return t('views.appInfo.findingBlockedByEnvironment')
        }
    }

    function findingDetail(finding: SystemFinding): string {
        switch (finding.kind) {
            case SystemFindingKind.NoDriverBound:
                return t('views.appInfo.findingNoDriverBound')
            case SystemFindingKind.Blacklisted:
                return t('views.appInfo.findingBlacklisted')
            case SystemFindingKind.SignatureRejected:
                return t('views.appInfo.findingSignatureRejected')
            case SystemFindingKind.BlockedByEnvironment:
                return blockerDetail(finding.reason)
            default:
                return t('views.appInfo.findingDetectionUnsupported')
        }
    }

    function noticeHeadline(kind: HardwareNoticeKind): string {
        switch (kind) {
            case 'blockedByEnvironment':
                return t('layout.shell.coolingPage.noticeBlockedByEnvironment')
            case 'noChannels':
                return t('layout.shell.coolingPage.noChannels')
            case 'noneControllable':
                return t('layout.shell.coolingPage.noneControllable')
        }
    }

    return { blockerDetail, findingDetail, noticeHeadline }
}
