// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import defaultHealthCheck, { type HealthDetails } from '@/models/HealthCheck.ts'
import {
    daemonStartedAt,
    DEBUG_LOGGING_SETTING_ROUTE,
    debugForcedBy,
    formatJournalSince,
    isDebugLogging,
    journalCommand,
    uptimeSeconds,
} from '@/shell/debugLogging.ts'
import { SETTINGS_ENTRIES } from '@/shell/search/settingsCatalog.ts'

describe('DEBUG_LOGGING_SETTING_ROUTE', () => {
    it('points at the indexed Settings row, so the indicators never dead-end', () => {
        const rowId = DEBUG_LOGGING_SETTING_ROUTE.params.tabNumber
        expect(SETTINGS_ENTRIES.some((entry) => entry.id === rowId)).toBe(true)
    })
})

const details = (overrides: Partial<HealthDetails>): HealthDetails => ({
    ...defaultHealthCheck().details,
    ...overrides,
})

describe('isDebugLogging', () => {
    it('counts DEBUG and TRACE as debug logging', () => {
        expect(isDebugLogging(details({ log_level: 'DEBUG' }))).toBe(true)
        expect(isDebugLogging(details({ log_level: 'TRACE' }))).toBe(true)
    })

    it('stays off for quieter levels and before the health check loads', () => {
        for (const level of ['INFO', 'WARN', 'ERROR', 'OFF', '']) {
            expect(isDebugLogging(details({ log_level: level }))).toBe(false)
        }
        expect(isDebugLogging(defaultHealthCheck().details)).toBe(false)
    })
})

describe('debugForcedBy', () => {
    it('names the env var or flag that turned debug on', () => {
        expect(debugForcedBy(details({ log_level: 'DEBUG', log_level_source: 'env' }))).toBe('env')
        expect(debugForcedBy(details({ log_level: 'TRACE', log_level_source: 'env' }))).toBe('env')
        expect(debugForcedBy(details({ log_level: 'DEBUG', log_level_source: 'flag' }))).toBe(
            'flag',
        )
    })

    it('leaves the switch unlocked when the setting or nothing turned debug on', () => {
        expect(
            debugForcedBy(details({ log_level: 'DEBUG', log_level_source: 'settings' })),
        ).toBeUndefined()
        // The packaged service sets CC_LOG=INFO, which must not lock the switch.
        expect(
            debugForcedBy(details({ log_level: 'INFO', log_level_source: 'env' })),
        ).toBeUndefined()
        expect(
            debugForcedBy(details({ log_level: 'INFO', log_level_source: 'default' })),
        ).toBeUndefined()
    })
})

describe('uptimeSeconds', () => {
    it('parses the daemon format, including hours past two digits', () => {
        expect(uptimeSeconds('00:00:00')).toBe(0)
        expect(uptimeSeconds('01:02:03')).toBe(3_723)
        expect(uptimeSeconds('123:59:59')).toBe(123 * 3_600 + 59 * 60 + 59)
    })

    it('rejects anything else', () => {
        for (const uptime of ['', '1:2:3', '00:60:00', '00:00:60', 'up', '00:00']) {
            expect(uptimeSeconds(uptime)).toBeUndefined()
        }
    })
})

describe('daemonStartedAt', () => {
    it('subtracts the uptime from the daemon clock', () => {
        const startedAt = daemonStartedAt('2026-09-25T12:00:00+02:00', '01:00:30')
        expect(startedAt?.toISOString()).toBe('2026-09-25T08:59:30.000Z')
    })

    it('is unknown until the health check loads', () => {
        expect(daemonStartedAt('', '')).toBeUndefined()
        expect(daemonStartedAt('2026-09-25T12:00:00Z', '')).toBeUndefined()
        expect(daemonStartedAt('not a date', '00:00:10')).toBeUndefined()
    })
})

describe('journalCommand', () => {
    it('formats --since in UTC, whatever the offset the daemon reported', () => {
        expect(formatJournalSince(new Date('2026-09-25T08:59:30+02:00'))).toBe(
            '2026-09-25 06:59:30 UTC',
        )
    })

    it('exports the daemon and plugin units from just before the start', () => {
        const command = journalCommand(new Date('2026-09-25T08:59:30Z'))
        expect(command).toBe(
            "journalctl --no-pager -u coolercontrold -u 'cc-plugin-*' " +
                '--since "2026-09-25 08:59:20 UTC" > coolercontrold-debug.log',
        )
    })

    it('falls back to the current boot without a start time', () => {
        expect(journalCommand(undefined)).toBe(
            "journalctl --no-pager -u coolercontrold -u 'cc-plugin-*' -b > coolercontrold-debug.log",
        )
    })
})
