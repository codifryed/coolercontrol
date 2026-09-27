// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

export interface HealthCheck {
    status: string
    description: string
    current_timestamp: string
    details: HealthDetails
    system: SystemDetails
    links: {
        docs: string
        repository: string
    }
}

/** Where the daemon's log level came from. */
export type LogLevelSource = 'default' | 'env' | 'flag' | 'settings'

export interface HealthDetails {
    uptime: string
    version: string
    pid: number
    memory_mb: number
    warnings: number
    errors: number
    liquidctl_connected: boolean
    /** The level the daemon started with, e.g. "INFO" or "DEBUG". */
    log_level: string
    log_level_source: LogLevelSource
    log_to_journal: boolean
}

export interface SystemDetails {
    name: string
}

export default function defaultHealthCheck(): HealthCheck {
    return {
        status: '',
        current_timestamp: '',
        description: '',
        details: {
            uptime: '',
            version: '',
            pid: 0,
            memory_mb: 0,
            warnings: 0,
            errors: 0,
            liquidctl_connected: false,
            log_level: 'INFO',
            log_level_source: 'default',
            log_to_journal: false,
        },
        system: {
            name: '',
        },
        links: {
            docs: '',
            repository: '',
        },
    }
}
