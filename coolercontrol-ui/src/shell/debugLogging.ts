// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { HealthDetails } from '@/models/HealthCheck.ts'

/** The Settings row that turns debug logging off, for the indicators to link to. */
export const DEBUG_LOGGING_SETTING_ROUTE = Object.freeze({
    name: 'settings',
    params: { tabNumber: 'setting-debug-logging' },
})

/** Covers collecting logs where there is no journal (OpenRC, containers, AppImage). */
export const LOGS_DOCS_URL = 'https://docs.coolercontrol.org/reference/logs-debugging.html'

// TRACE is louder still, so it counts as debug logging too.
const DEBUG_LEVELS: ReadonlySet<string> = new Set(['DEBUG', 'TRACE'])

// Uptime is whole seconds rounded down, so the real start can be up to a second
// before the estimate. The slack keeps the first debug lines in the export.
export const SINCE_MARGIN_SECONDS = 10

export function isDebugLogging(details: HealthDetails): boolean {
    return DEBUG_LEVELS.has(details.log_level)
}

/** The env var or flag that turned debug on, which the setting cannot turn off. */
export function debugForcedBy(details: HealthDetails): 'env' | 'flag' | undefined {
    if (!isDebugLogging(details)) return undefined
    const source = details.log_level_source
    if (source === 'env' || source === 'flag') return source
    return undefined
}

/** Parses the daemon's `hh:mm:ss` uptime. Hours can run past two digits. */
export function uptimeSeconds(uptime: string): number | undefined {
    const match = /^(\d+):([0-5]\d):([0-5]\d)$/.exec(uptime)
    if (match == null) return undefined
    return Number(match[1]) * 3_600 + Number(match[2]) * 60 + Number(match[3])
}

/** Uses the daemon's own clock, so a browser on another machine cannot skew it. */
export function daemonStartedAt(currentTimestamp: string, uptime: string): Date | undefined {
    const nowMs = Date.parse(currentTimestamp)
    const seconds = uptimeSeconds(uptime)
    if (Number.isNaN(nowMs) || seconds == null) return undefined
    return new Date(nowMs - seconds * 1_000)
}

/** journalctl's `--since` format, in UTC so the daemon host's timezone does not matter. */
export function formatJournalSince(date: Date): string {
    return `${date.toISOString().slice(0, 19).replace('T', ' ')} UTC`
}

/**
 * Exports everything logged since the daemon started, which is when debug began, since the
 * level only changes on a restart. Plugins log to their own units. Without a start time it
 * falls back to the current boot.
 */
export function journalCommand(startedAt: Date | undefined): string {
    const units = "-u coolercontrold -u 'cc-plugin-*'"
    const since =
        startedAt == null
            ? '-b'
            : `--since "${formatJournalSince(new Date(startedAt.getTime() - SINCE_MARGIN_SECONDS * 1_000))}"`
    return `journalctl --no-pager ${units} ${since} > coolercontrold-debug.log`
}
