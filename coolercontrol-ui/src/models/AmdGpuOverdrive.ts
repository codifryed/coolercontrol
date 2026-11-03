// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

export type OverdriveState = 'not_configured' | 'applying' | 'reboot_required' | 'failed' | 'active'

export type OverdriveAction = 'enable' | 'disable'

/**
 * The AMD GPU overdrive setup state, one for the whole system. Served by
 * `GET /amd-gpu-overdrive` and pushed on the SSE `amd-gpu-overdrive` event.
 */
export class AmdGpuOverdriveStatus {
    state: OverdriveState = 'not_configured'

    tool: string | null = null

    manual_command: string | null = null

    error: string | null = null

    rebuild_skipped: boolean = false

    /** What the current or last run since the daemon started does. Null before any. */
    action: OverdriveAction | null = null

    /** CoolerControl's modprobe conf is in place, so Disable can remove it. */
    removable: boolean = false
}

export type OverdriveRoute = 'rebuild' | 'kernel_argument' | 'config_only'

/**
 * What enabling or disabling would do on this system, from `GET /amd-gpu-overdrive/plan`, shown
 * for the user to confirm first.
 */
export class AmdGpuOverdrivePlan {
    route: OverdriveRoute | null = null

    tool: string | null = null

    command: string | null = null

    files: string[] = []

    problem: string | null = null
}

/**
 * Whether an enable or disable response is still news. The SSE event of a rebuild that fails at
 * once can overtake it, and applying the older response would put the state back to applying for
 * good. Every update replaces the status object, so an unchanged object means nothing came first.
 */
export function responseIsCurrent(
    atRequest: AmdGpuOverdriveStatus | undefined,
    current: AmdGpuOverdriveStatus | undefined,
): boolean {
    return atRequest === current
}

const KEYS = 'components.deviceExtensionSettings.'

/**
 * Every key this file builds. They are put together at runtime, so the shell's static key sweep
 * cannot see them, and it walks this list instead.
 */
export const OVERDRIVE_I18N_KEYS: readonly string[] = [
    'overdriveActive',
    'overdriveApplying',
    'overdriveEnable',
    'overdriveFailed',
    'overdriveFailedDetail',
    'overdriveFailedError',
    'overdriveFailedSummary',
    'overdrivePlanConfigOnly',
    'overdrivePlanKernelArgument',
    'overdrivePlanRebuild',
    'overdrivePlanRebuildOnly',
    'overdrivePlanRecovery',
    'overdrivePlanRecoveryDeployment',
    'overdrivePlanRemoveConfigOnly',
    'overdrivePlanRemoveRebuild',
    'overdrivePlanRemoveRecovery',
    'overdriveRebootRequired',
    'overdriveRebootToTurnOff',
    'overdriveRebuildSkipped',
    'overdriveRebuildStillRunning',
    'overdriveRebuildStillRunningSummary',
    'overdriveRebuildYourself',
    'overdriveRemoveFailedDetail',
    'overdriveRemoveFailedSummary',
    'overdriveRemovedDetail',
    'overdriveRemovedNowDetail',
    'overdriveRemovedRebuildSkipped',
    'overdriveRemovedSummary',
    'overdriveRemoving',
    'overdriveSuccess',
    'overdriveSuccessDetail',
].map((key) => `${KEYS}${key}`)

export interface OverdriveMessage {
    key: string
    params: Record<string, string>
}

export interface OverdriveToast {
    severity: 'success' | 'warn' | 'error'
    summaryKey: string
    detail: OverdriveMessage
}

/**
 * The state the row shows. Before the first load it falls back to the device info, which only
 * knows active or not.
 */
export function overdriveRowState(
    status: AmdGpuOverdriveStatus | undefined,
    infoActive: boolean,
): OverdriveState {
    if (status == null) return infoActive ? 'active' : 'not_configured'
    return status.state
}

/** What the user should know about the current state, if anything beyond its label. */
export function overdriveHint(status: AmdGpuOverdriveStatus): OverdriveMessage | undefined {
    const disabling = status.action === 'disable'
    const command = status.manual_command ?? ''
    switch (status.state) {
        case 'failed': {
            if (command.length === 0) {
                return { key: `${KEYS}overdriveFailedError`, params: { error: status.error ?? '' } }
            }
            const key = disabling ? 'overdriveRemoveFailedDetail' : 'overdriveFailedDetail'
            return { key: `${KEYS}${key}`, params: { error: status.error ?? '', command } }
        }
        case 'reboot_required':
            if (disabling) {
                const key = status.rebuild_skipped
                    ? 'overdriveRemovedRebuildSkipped'
                    : 'overdriveRemovedDetail'
                return { key: `${KEYS}${key}`, params: {} }
            }
            // With a command, the user chose to rebuild it themselves. Without, no tool was found.
            if (status.rebuild_skipped && command.length > 0) {
                return { key: `${KEYS}overdriveRebuildYourself`, params: { command } }
            }
            if (status.rebuild_skipped) return { key: `${KEYS}overdriveRebuildSkipped`, params: {} }
            return { key: `${KEYS}overdriveSuccessDetail`, params: {} }
        case 'applying':
            // An error while applying means the rebuild outlasted the wait and goes on alone.
            if (status.error == null) return undefined
            return { key: `${KEYS}overdriveRebuildStillRunning`, params: {} }
        case 'not_configured':
            return disabling ? { key: `${KEYS}overdriveRemovedNowDetail`, params: {} } : undefined
        default:
            return undefined
    }
}

/** The row's label, which says which way a run goes. */
export function overdriveStateLabelKey(
    state: OverdriveState,
    action: OverdriveAction | null | undefined,
): string {
    const disabling = action === 'disable'
    switch (state) {
        case 'applying':
            return disabling ? `${KEYS}overdriveRemoving` : `${KEYS}overdriveApplying`
        case 'reboot_required':
            return disabling ? `${KEYS}overdriveRebootToTurnOff` : `${KEYS}overdriveRebootRequired`
        case 'failed':
            return `${KEYS}overdriveFailed`
        case 'active':
            return `${KEYS}overdriveActive`
        case 'not_configured':
            return `${KEYS}overdriveEnable`
    }
}

/** What the Enable or Retry button does: Retry repeats a failed disable, and all else enables. */
export function overdriveButtonAction(
    state: OverdriveState,
    action: OverdriveAction | null | undefined,
): OverdriveAction {
    return state === 'failed' && action === 'disable' ? 'disable' : 'enable'
}

/** Whether to offer Disable: CoolerControl's conf is there and nothing is running. */
export function overdriveCanDisable(status: AmdGpuOverdriveStatus | undefined): boolean {
    return status != null && status.removable && status.state !== 'applying'
}

/** Whether the change takes a while and only applies after a reboot. */
export function overdrivePlanTakesTime(plan: AmdGpuOverdrivePlan): boolean {
    return plan.route === 'rebuild' || plan.route === 'kernel_argument'
}

/** The dialog's account of what the change will do. Undefined for a plan that cannot run. */
export function overdrivePlanIntro(
    plan: AmdGpuOverdrivePlan,
    action: OverdriveAction,
): OverdriveMessage | undefined {
    const files = plan.files.join(', ')
    const disable = action === 'disable'
    switch (plan.route) {
        case 'rebuild':
            // A retried disable finds its files already gone, so only the rebuild is left.
            if (plan.files.length === 0)
                return { key: `${KEYS}overdrivePlanRebuildOnly`, params: {} }
            return {
                key: `${KEYS}${disable ? 'overdrivePlanRemoveRebuild' : 'overdrivePlanRebuild'}`,
                params: { files },
            }
        case 'kernel_argument':
            return { key: `${KEYS}overdrivePlanKernelArgument`, params: {} }
        case 'config_only':
            return {
                key: `${KEYS}${disable ? 'overdrivePlanRemoveConfigOnly' : 'overdrivePlanConfigOnly'}`,
                params: { files },
            }
        default:
            return undefined
    }
}

/** How to get back to a system that boots, should the change keep it from booting. */
export function overdrivePlanRecovery(
    plan: AmdGpuOverdrivePlan,
    action: OverdriveAction,
): OverdriveMessage | undefined {
    if (plan.route === 'kernel_argument')
        return { key: `${KEYS}overdrivePlanRecoveryDeployment`, params: {} }
    if (plan.route == null) return undefined
    if (action === 'disable') return { key: `${KEYS}overdrivePlanRemoveRecovery`, params: {} }
    if (plan.files.length === 0) return undefined
    return { key: `${KEYS}overdrivePlanRecovery`, params: { conf: plan.files[0] } }
}

/** The toast for a status a run ended in. Undefined for a state that is no outcome. */
export function overdriveOutcomeToast(status: AmdGpuOverdriveStatus): OverdriveToast | undefined {
    if (status.state === 'active') return undefined
    const detail = overdriveHint(status)
    if (detail == null) return undefined
    if (status.state === 'applying') {
        return {
            severity: 'warn',
            summaryKey: `${KEYS}overdriveRebuildStillRunningSummary`,
            detail,
        }
    }
    const disabling = status.action === 'disable'
    if (status.state === 'failed') {
        const summary = disabling ? 'overdriveRemoveFailedSummary' : 'overdriveFailedSummary'
        return { severity: 'error', summaryKey: `${KEYS}${summary}`, detail }
    }
    return {
        severity: status.rebuild_skipped ? 'warn' : 'success',
        summaryKey: `${KEYS}${disabling ? 'overdriveRemovedSummary' : 'overdriveSuccess'}`,
        detail,
    }
}

/**
 * Whether a pushed status ends a background run. A rebuild that outlasts the wait stays
 * applying with an error, and counts once, when the error first shows.
 */
export function overdriveEventIsOutcome(
    previous: AmdGpuOverdriveStatus | undefined,
    status: AmdGpuOverdriveStatus,
): boolean {
    if (previous?.state !== 'applying') return false
    if (status.state !== 'applying') return true
    return previous.error == null && status.error != null
}

/**
 * Whether an enable or disable response ended within its request and changed something, so it
 * gets its toast there. A background run gets its toast from the event that ends it instead.
 */
export function overdriveResponseIsOutcome(
    atRequest: AmdGpuOverdriveStatus | undefined,
    response: AmdGpuOverdriveStatus,
): boolean {
    if (response.state === 'applying') return false
    return atRequest?.state !== response.state || atRequest?.action !== response.action
}
