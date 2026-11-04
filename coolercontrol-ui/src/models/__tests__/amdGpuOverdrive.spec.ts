// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import en from '@/i18n/locales/en.ts'
import {
    AmdGpuOverdrivePlan,
    AmdGpuOverdriveStatus,
    OVERDRIVE_I18N_KEYS,
    type OverdriveMessage,
    overdriveButtonAction,
    overdriveCanDisable,
    overdriveEventIsOutcome,
    overdriveHint,
    overdriveMessageText,
    overdriveOutcomeToast,
    overdrivePlanFiles,
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

const keyOf = (message: OverdriveMessage | undefined): string | undefined =>
    message != null && 'key' in message ? message.key : undefined

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
        expect(overdriveHint(removal)).toEqual(overdriveHint(failed))
    })

    it('tells the user a rebuild that outlasted the wait is still running', () => {
        expect(overdriveHint(status({ state: 'applying' }))).toBeUndefined()
        const stillRunning = status({ state: 'applying', error: 'still running' })
        expect(overdriveHint(stillRunning)).toEqual({
            key: `${KEYS}overdriveRebuildStillRunning`,
            params: {},
        })
    })

    it('shows the bare error when there is no command', () => {
        expect(overdriveHint(status({ state: 'failed', error: 'rpm-ostree failed' }))).toEqual({
            text: 'rpm-ostree failed',
        })
    })

    it('tells a skipped rebuild apart from an unknown tool', () => {
        const yourself = status({
            state: 'reboot_required',
            rebuild_skipped: true,
            manual_command: '/usr/bin/dracut --regenerate-all --force',
        })
        expect(keyOf(overdriveHint(yourself))).toBe(`${KEYS}overdriveRebuildYourself`)
        const unknown = status({ state: 'reboot_required', rebuild_skipped: true })
        expect(keyOf(overdriveHint(unknown))).toBe(`${KEYS}overdriveRebuildSkipped`)
        const done = status({ state: 'reboot_required' })
        expect(keyOf(overdriveHint(done))).toBe(`${KEYS}overdriveSuccessDetail`)
    })

    it('words a finished removal like a finished setup', () => {
        const pending = status({ state: 'reboot_required', action: 'disable' })
        expect(keyOf(overdriveHint(pending))).toBe(`${KEYS}overdriveSuccessDetail`)
        const skipped = status({ ...pending, rebuild_skipped: true })
        expect(keyOf(overdriveHint(skipped))).toBe(`${KEYS}overdriveRebuildSkipped`)
        const off = status({ state: 'not_configured', action: 'disable' })
        expect(overdriveHint(off)).toBeUndefined()
    })
})

describe('overdriveMessageText', () => {
    it('translates a key and passes daemon text through', () => {
        const t = (key: string, params: Record<string, string>) => `${key}|${params.command}`
        expect(overdriveMessageText({ key: 'k', params: { command: 'c' } }, t)).toBe('k|c')
        expect(overdriveMessageText({ text: 'No space left' }, t)).toBe('No space left')
    })
})

describe('labels and buttons', () => {
    it('labels each state', () => {
        expect(overdriveStateLabelKey('applying')).toBe(`${KEYS}overdriveApplying`)
        expect(overdriveStateLabelKey('reboot_required')).toBe(`${KEYS}overdriveRebootRequired`)
        expect(overdriveStateLabelKey('not_configured')).toBe(`${KEYS}overdriveEnable`)
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

    it('lists the files written or deleted', () => {
        const rebuild = plan({ route: 'rebuild', command: '/usr/bin/mkinitcpio -P', files })
        expect(overdrivePlanFiles(rebuild, 'enable')).toEqual({
            key: `${KEYS}overdrivePlanWrites`,
            params: { files: files[0] },
        })
        expect(keyOf(overdrivePlanFiles(rebuild, 'disable'))).toBe(`${KEYS}overdrivePlanDeletes`)
    })

    it('lists no files where there are none or the plan cannot run', () => {
        const retry = plan({ route: 'rebuild', command: '/usr/bin/mkinitcpio -P' })
        expect(overdrivePlanFiles(retry, 'disable')).toBeUndefined()
        const karg = plan({ route: 'kernel_argument', command: '/usr/bin/rpm-ostree kargs' })
        expect(overdrivePlanFiles(karg, 'enable')).toBeUndefined()
        expect(overdrivePlanFiles(plan({ problem: 'NixOS', files }), 'enable')).toBeUndefined()
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
            params: {},
        })
        expect(overdrivePlanRecovery(rebuild, 'disable')).toEqual(
            overdrivePlanRecovery(rebuild, 'enable'),
        )
        expect(keyOf(overdrivePlanRecovery(plan({ route: 'kernel_argument' }), 'enable'))).toBe(
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
            summaryKey: `${KEYS}overdriveFailedSummary`,
        })
        const off = overdriveOutcomeToast(status({ state: 'not_configured', action: 'disable' }))
        expect(off).toEqual({
            severity: 'success',
            summaryKey: `${KEYS}overdriveRemovedSummary`,
            detail: undefined,
        })
    })

    it('warns that a rebuild that outlasted the wait is still running', () => {
        const stillRunning = overdriveOutcomeToast(
            status({ state: 'applying', action: 'enable', error: 'still running' }),
        )
        expect(stillRunning).toMatchObject({
            severity: 'warn',
            summaryKey: `${KEYS}overdriveRebuildStillRunningSummary`,
            detail: { key: `${KEYS}overdriveRebuildStillRunning` },
        })
    })

    it('toasts an event only when it ends a run or first reports it still running', () => {
        const running = status({ state: 'applying' })
        const stillRunning = status({ state: 'applying', error: 'still running' })
        expect(overdriveEventIsOutcome(running, status({ state: 'reboot_required' }))).toBe(true)
        expect(overdriveEventIsOutcome(running, stillRunning)).toBe(true)
        expect(overdriveEventIsOutcome(stillRunning, stillRunning)).toBe(false)
        expect(overdriveEventIsOutcome(running, running)).toBe(false)
        expect(overdriveEventIsOutcome(undefined, stillRunning)).toBe(false)
        expect(overdriveEventIsOutcome(status({ state: 'failed' }), running)).toBe(false)
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
