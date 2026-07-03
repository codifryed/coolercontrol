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

/**
 * The daemon's sparse user-defined name overrides document (overrides.toml).
 * Devices and channels without an override are absent.
 */
export interface NameOverrides {
    devices: Record<UID, DeviceNameOverrides>
}

export interface DeviceNameOverrides {
    /** Daemon-written detected-name hint for hand-editors. */
    device_name?: string
    /** The user-defined device display name. */
    name?: string
    channels?: Record<string, ChannelNameOverrides>
}

export interface ChannelNameOverrides {
    /** The user-defined channel display label. */
    label?: string
}
