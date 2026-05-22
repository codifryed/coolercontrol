/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
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

import type { UID } from '@/models/Device'

export interface ChannelStats {
    min: number
    max: number
    avg: number
    count: number
}

// Wire discriminator from the daemon's ChannelDataType (SCREAMING_SNAKE_CASE).
// Temps are tracked separately under `temps` and so are not represented here.
export type ChannelStatField = 'DUTY' | 'RPM' | 'FREQ' | 'WATTS'

export interface DeviceStatsDTO {
    uid: UID
    temps: Record<string, ChannelStats>
    channels: Record<string, Partial<Record<ChannelStatField, ChannelStats>>>
}

export interface StatsResponseDTO {
    devices: DeviceStatsDTO[]
}

export function defaultStatsResponse(): StatsResponseDTO {
    return { devices: [] }
}
