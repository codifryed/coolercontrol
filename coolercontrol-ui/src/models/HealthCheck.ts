/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

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

export interface HealthDetails {
    uptime: string
    version: string
    pid: number
    memory_mb: number
    warnings: number
    errors: number
    liquidctl_connected: boolean
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
