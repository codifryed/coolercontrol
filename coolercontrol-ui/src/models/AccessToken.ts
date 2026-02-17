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

export interface AccessTokenInfo {
    id: string
    label: string
    created_at: string
    expires_at: string | null
    last_used: string | null
}

export interface CreateTokenRequest {
    label: string
    expires_at: string | null
}

export interface CreateTokenResponse {
    id: string
    label: string
    token: string
    created_at: string
    expires_at: string | null
}
