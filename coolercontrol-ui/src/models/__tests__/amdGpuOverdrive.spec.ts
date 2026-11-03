// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import en from '@/i18n/locales/en.ts'
import {
    AmdGpuOverdrivePlan,
    AmdGpuOverdriveStatus,
    OVERDRIVE_I18N_KEYS,
    overdriveButtonAction,
    overdriveCanDisable,
    overdriveHint,
    overdriveOutcomeToast,
    overdrivePlanIntro,
    overdrivePlanRecovery,
    overdrivePlanTakesTime,
    overdriveResponseIsOutcome,
    overdriveRowState,
    overdriveStateLabelKey,
    responseIsCurrent,
} from '@/models/AmdGpuOverdrive.ts'

const status = (fields: Partial<AmdGpuOverdriveStatus>): AmdGpuOverdriveStatus =>
    Object.assign(new AmdGpuOverdriveStatus(), fields)

const plan = (fields: Partial<AmdGpuOverdrivePlan>): AmdGpuOverdrivePlan =>
    Object.assign(new AmdGpuOverdrivePlan(), fields)

const resolve = (key: string): unknown =>
    key.split('.').reduce<unknown>((node, part) => (node as Record<string, unknown>)?.[part], en)

const KEYS = 'components.deviceExtensionSettings.'

describe('overdriveRowState', () => {
    it('falls back to the device info without a status', () => {
        expect(overdriveRowState(undefined, true)).toBe('active')
        expect(overdriveRowState(undefined, false)).toBe('not_configured')
    })

    it('follows the status when there is one', () => {
        expect(overdriveRowState(status({ state: 'applying' }), false)).toBe('applying')
    })
})

describe('overdriveHint', () => {
    it('hands a failure the command to finish by hand', () => {
        const failed = status({
            state: 'failed',
            error: 'no space',
            manual_command: '/usr/bin/mkinitcpio -P',
        })
        expect(overdriveHint(failed)).toEqual({
            key: `${KEYS}overdriveFailedDetail`,
            params: { error: 'no space', command: '/usr/bin/mkinitcpio -P' },
        })
        const removal = status({ ...failed, action: 'disable' })
        expect(overdriveHint(removal)?.key).toBe(`${KEYS}overdriveRemoveFailedDetail`)
    })

    it('shows the bare error when there is no command', () => {
        expect(overdriveHint(status({ state: 'failed', error: 'rpm-ostree failed' }))).toEqual({
            key: `${KEYS}overdriveFailedError`,
            params: { error: 'rpm-ostree failed' },
        })
    })

    it('tells a skipped rebuild apart from an unknown tool', () => {
        const yourself = status({
            state: 'reboot_required',
            rebuild_skipped: true,
            manual_command: '/usr/bin/dracut --regenerate-all --force',
        })
        expect(overdriveHint(yourself)?.key).toBe(`${KEYS}overdriveRebuildYourself`)
        const unknown = status({ state: 'reboot_required', rebuild_skipped: true })
        expect(overdriveHint(unknown)?.key).toBe(`${KEYS}overdriveRebuildSkipped`)
        const done = status({ state: 'reboot_required' })
        expect(overdriveHint(done)?.key).toBe(`${KEYS}overdriveSuccessDetail`)
    })

    it('explains a finished removal', () => {
        const pending = status({ state: 'reboot_required', action: 'disable' })
        expect(overdriveHint(pending)?.key).toBe(`${KEYS}overdriveRemovedDetail`)
        const off = status({ state: 'not_configured', action: 'disable' })
        expect(overdriveHint(off)?.key).toBe(`${KEYS}overdriveRemovedNowDetail`)
        expect(overdriveHint(status({ state: 'not_configured' }))).toBeUndefined()
    })
})

describe('labels and buttons', () => {
    it('says which way a run goes', () => {
        expect(overdriveStateLabelKey('applying', 'enable')).toBe(`${KEYS}overdriveApplying`)
        expect(overdriveStateLabelKey('applying', 'disable')).toBe(`${KEYS}overdriveRemoving`)
        expect(overdriveStateLabelKey('reboot_required', 'disable')).toBe(
            `${KEYS}overdriveRebootToTurnOff`,
        )
    })

    it('retries the action that failed', () => {
        expect(overdriveButtonAction('failed', 'disable')).toBe('disable')
        expect(overdriveButtonAction('failed', 'enable')).toBe('enable')
        expect(overdriveButtonAction('not_configured', null)).toBe('enable')
    })

    it('offers Disable while the conf is there and nothing runs', () => {
        expect(overdriveCanDisable(status({ state: 'active', removable: true }))).toBe(true)
        expect(overdriveCanDisable(status({ state: 'failed', removable: true }))).toBe(true)
        expect(overdriveCanDisable(status({ state: 'applying', removable: true }))).toBe(false)
        expect(overdriveCanDisable(status({ state: 'active', removable: false }))).toBe(false)
        expect(overdriveCanDisable(undefined)).toBe(false)
    })
})

describe('plan text', () => {
    const files = ['/etc/modprobe.d/99-amdgpu-overdrive.conf']

    it('describes each route', () => {
        const rebuild = plan({ route: 'rebuild', command: '/usr/bin/mkinitcpio -P', files })
        expect(overdrivePlanIntro(rebuild, 'enable')).toEqual({
            key: `${KEYS}overdrivePlanRebuild`,
            params: { files: files[0] },
        })
        expect(overdrivePlanIntro(rebuild, 'disable')?.key).toBe(
            `${KEYS}overdrivePlanRemoveRebuild`,
        )
        const karg = plan({ route: 'kernel_argument', command: '/usr/bin/rpm-ostree kargs' })
        expect(overdrivePlanIntro(karg, 'enable')?.key).toBe(`${KEYS}overdrivePlanKernelArgument`)
        const configOnly = plan({ route: 'config_only', files })
        expect(overdrivePlanIntro(configOnly, 'disable')?.key).toBe(
            `${KEYS}overdrivePlanRemoveConfigOnly`,
        )
        expect(overdrivePlanIntro(plan({ problem: 'NixOS' }), 'enable')).toBeUndefined()
    })

    it('shows only the rebuild for a retried disable whose files are gone', () => {
        const retry = plan({ route: 'rebuild', command: '/usr/bin/mkinitcpio -P' })
        expect(overdrivePlanIntro(retry, 'disable')?.key).toBe(`${KEYS}overdrivePlanRebuildOnly`)
    })

    it('warns about the wait only where a command runs', () => {
        expect(overdrivePlanTakesTime(plan({ route: 'rebuild' }))).toBe(true)
        expect(overdrivePlanTakesTime(plan({ route: 'kernel_argument' }))).toBe(true)
        expect(overdrivePlanTakesTime(plan({ route: 'config_only' }))).toBe(false)
    })

    it('names the way back to a booting system', () => {
        const rebuild = plan({ route: 'rebuild', files })
        expect(overdrivePlanRecovery(rebuild, 'enable')).toEqual({
            key: `${KEYS}overdrivePlanRecovery`,
            params: { conf: files[0] },
        })
        expect(overdrivePlanRecovery(rebuild, 'disable')?.key).toBe(
            `${KEYS}overdrivePlanRemoveRecovery`,
        )
        expect(overdrivePlanRecovery(plan({ route: 'kernel_argument' }), 'enable')?.key).toBe(
            `${KEYS}overdrivePlanRecoveryDeployment`,
        )
        expect(overdrivePlanRecovery(plan({ problem: 'NixOS' }), 'enable')).toBeUndefined()
    })
})

describe('toasts', () => {
    it('gives each outcome its severity and summary', () => {
        const done = overdriveOutcomeToast(status({ state: 'reboot_required', action: 'enable' }))
        expect(done).toMatchObject({ severity: 'success', summaryKey: `${KEYS}overdriveSuccess` })
        const skipped = overdriveOutcomeToast(
            status({ state: 'reboot_required', action: 'enable', rebuild_skipped: true }),
        )
        expect(skipped?.severity).toBe('warn')
        const failed = overdriveOutcomeToast(status({ state: 'failed', action: 'disable' }))
        expect(failed).toMatchObject({
            severity: 'error',
            summaryKey: `${KEYS}overdriveRemoveFailedSummary`,
        })
        const off = overdriveOutcomeToast(status({ state: 'not_configured', action: 'disable' }))
        expect(off?.summaryKey).toBe(`${KEYS}overdriveRemovedSummary`)
    })

    it('has no toast for a state that is no outcome', () => {
        expect(overdriveOutcomeToast(status({ state: 'applying' }))).toBeUndefined()
        expect(overdriveOutcomeToast(status({ state: 'active' }))).toBeUndefined()
        expect(overdriveOutcomeToast(status({ state: 'not_configured' }))).toBeUndefined()
    })

    it('toasts a response only when it ended at once and changed something', () => {
        const before = status({ state: 'not_configured' })
        const configured = status({ state: 'reboot_required', action: 'enable' })
        expect(overdriveResponseIsOutcome(before, configured)).toBe(true)
        expect(overdriveResponseIsOutcome(before, status({ state: 'applying' }))).toBe(false)
        // A repeated enable finds the setting staged and changes nothing.
        expect(overdriveResponseIsOutcome(configured, configured)).toBe(false)
    })

    it('keeps a response that an event overtook from winding the state back', () => {
        const atRequest = status({ state: 'not_configured' })
        expect(responseIsCurrent(atRequest, atRequest)).toBe(true)
        expect(responseIsCurrent(atRequest, status({ state: 'failed' }))).toBe(false)
    })
})

describe('OVERDRIVE_I18N_KEYS', () => {
    it('lists only keys the English locale defines', () => {
        for (const key of OVERDRIVE_I18N_KEYS) {
            expect(typeof resolve(key), key).toBe('string')
        }
    })
})
