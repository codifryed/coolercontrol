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

import { UID } from '@/models/Device.ts'
import { Type } from 'class-transformer'
import { ChannelSource } from '@/models/ChannelSource.ts'
import { v4 as uuidV4 } from 'uuid'
import i18n from '@/i18n'

export class Alert {
    uid: UID = uuidV4()
    name: string
    channel_source: ChannelSource
    min: number
    max: number
    state?: AlertState

    constructor(name: string, channel_source: ChannelSource, min: number, max: number) {
        this.name = name
        this.channel_source = channel_source
        this.min = min
        this.max = max
    }
}

export enum AlertState {
    Active = 'Active',
    Inactive = 'Inactive',
}

/**
 * 获取AlertState的本地化显示名称
 * @param state AlertState枚举值
 * @returns 本地化的显示名称
 */
export function getAlertStateDisplayName(state: AlertState): string {
    const { t } = i18n.global
    switch (state) {
        case AlertState.Active:
            return t('models.alertState.active')
        case AlertState.Inactive:
            return t('models.alertState.inactive')
        default:
            return String(state)
    }
}

export class AlertsDTO {
    @Type(() => Alert)
    alerts: Array<Alert> = []
    @Type(() => AlertLog)
    logs: Array<AlertLog> = []
}

export class AlertLog {
    uid: UID
    name: string
    state: AlertState
    message: string
    timestamp: string

    constructor(uid: UID, name: string, state: AlertState, message: string, timestamp: string) {
        this.uid = uid
        this.name = name
        this.state = state
        this.message = message
        this.timestamp = timestamp
    }
}
