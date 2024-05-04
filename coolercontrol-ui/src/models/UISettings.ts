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

import type { Color } from '@/models/Device'
import { Exclude, Type } from 'class-transformer'
import type { UID } from '@/models/Device'

/**
 * A DTO Class to hold all the UI settings to be persisted by the daemon.
 * The Class-Transformer has issues with Maps, so we have to use Arrays to
 * store that data and do the transformation.
 */
export class UISettingsDTO {
    devices: Array<UID> | undefined = []
    @Type(() => DeviceUISettingsDTO)
    deviceSettings: Array<DeviceUISettingsDTO> | undefined = []
    systemOverviewOptions: SystemOverviewOptions | undefined
    startInSystemTray: boolean = false
    closeToSystemTray: boolean = false
    displayHiddenItems: boolean = true
    themeMode: ThemeMode = ThemeMode.SYSTEM
    uiScale: number = 100
    menuMode: string = 'static'
    time24: boolean = false
}

export enum ThemeMode {
    SYSTEM = 'system',
    DARK = 'dark',
    LIGHT = 'light',
    HIGH_CONTRAST_DARK = 'high-contrast-dark',
    HIGH_CONTRAST_LIGHT = 'high-contrast-light',
}

export class DeviceUISettingsDTO {
    menuCollapsed: boolean = false
    userName: string | undefined
    names: Array<string> = []
    @Type(() => SensorAndChannelSettings)
    sensorAndChannelSettings: Array<SensorAndChannelSettings> = []
}

export interface SystemOverviewOptions {
    selectedTimeRange: {
        name: string
        seconds: number
    }
    selectedChartType: string
    temp: boolean
    duty: boolean
    load: boolean
    rpm: boolean
    freq: boolean
    timeChartLineScale: number
}

export type AllDeviceSettings = Map<UID, DeviceUISettings>

/**
 * A Device's Settings
 */
export class DeviceUISettings {
    /**
     * Whether the main menu's Device entry is collapsed or not
     */
    menuCollapsed: boolean = false
    displayName: string = ''
    userName: string | undefined

    /**
     * A Map of Sensor and Channel Names to associated Settings.
     */
    readonly sensorsAndChannels: Map<string, SensorAndChannelSettings> = new Map()

    get name(): string {
        return this.userName == null ? this.displayName : this.userName
    }
}

export class SensorAndChannelSettings {
    @Exclude() // we don't want to persist this, it should be generated anew on each start
    defaultColor: Color

    userColor: Color | undefined
    hide: boolean

    @Exclude() // we don't want to persist this
    displayName: string = ''
    userName: string | undefined

    constructor(defaultColor: Color = '#568af2', hide: boolean = false) {
        this.defaultColor = defaultColor
        this.hide = hide
    }

    get color(): Color {
        return this.userColor != null ? this.userColor : this.defaultColor
    }

    get name(): string {
        return this.userName != null ? this.userName : this.displayName
    }
}
