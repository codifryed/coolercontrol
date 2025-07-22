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

import type { Color } from '@/models/Device'
import { Exclude, Type } from 'class-transformer'
import type { UID } from '@/models/Device'
import { Dashboard } from '@/models/Dashboard.ts'
import i18n from '@/i18n'

/**
 * A DTO Class to hold all the UI settings to be persisted by the daemon.
 * The Class-Transformer has issues with Maps, so we have to use Arrays to
 * store that data and do the transformation.
 */
export class UISettingsDTO {
    devices?: Array<UID> = []

    @Type(() => DeviceUISettingsDTO)
    deviceSettings?: Array<DeviceUISettingsDTO> = []

    @Type(() => Dashboard)
    dashboards: Array<Dashboard> = []
    homeDashboard?: UID
    themeMode: ThemeMode = ThemeMode.SYSTEM
    chartLineScale: number = 1.5
    time24: boolean = false
    collapsedMenuNodeIds: Array<string> = []
    collapsedMainMenu: boolean = false
    hideMenuCollapseIcon: boolean = false
    menuEntitiesAtBottom: boolean = false
    mainMenuWidthRem: number = 24
    frequencyPrecision: number = 1
    customTheme: CustomThemeSettings = {
        accent: defaultCustomTheme.accent,
        bgOne: defaultCustomTheme.bgOne,
        bgTwo: defaultCustomTheme.bgTwo,
        borderOne: defaultCustomTheme.borderOne,
        textColor: defaultCustomTheme.textColor,
        textColorSecondary: defaultCustomTheme.textColorSecondary,
    }
    showOnboarding: boolean = true
}

export enum ThemeMode {
    SYSTEM = 'system',
    DARK = 'dark',
    LIGHT = 'light',
    HIGH_CONTRAST_DARK = 'high-contrast-dark',
    HIGH_CONTRAST_LIGHT = 'high-contrast-light',
    CUSTOM = 'custom theme',
}

/**
 * 获取ThemeMode的本地化显示名称
 * @param mode ThemeMode枚举值
 * @returns 本地化的显示名称
 */
export function getThemeModeDisplayName(mode: ThemeMode): string {
    const { t } = i18n.global
    switch (mode) {
        case ThemeMode.SYSTEM:
            return t('models.themeMode.system')
        case ThemeMode.DARK:
            return t('models.themeMode.dark')
        case ThemeMode.LIGHT:
            return t('models.themeMode.light')
        case ThemeMode.HIGH_CONTRAST_DARK:
            return t('models.themeMode.highContrastDark')
        case ThemeMode.HIGH_CONTRAST_LIGHT:
            return t('models.themeMode.highContrastLight')
        case ThemeMode.CUSTOM:
            return t('models.themeMode.custom')
        default:
            return String(mode)
    }
}

export interface CustomThemeSettings {
    accent: Color
    bgOne: Color
    bgTwo: Color
    borderOne: Color
    textColor: Color
    textColorSecondary: Color
}
export const defaultCustomTheme: CustomThemeSettings = {
    // default dark-theme
    accent: '86 138 242', //'#568af2'
    bgOne: '27 30 35', //'#1b1e23'
    bgTwo: '44 49 60', //'#2c313c'
    borderOne: '138 149 170 0.25', //'#8a95aa40'
    textColor: '220 225 236', //'#dce1ec'
    textColorSecondary: '138 149 170', //'#8a95aa'
}

export class DeviceUISettingsDTO {
    menuCollapsed: boolean = false
    userName?: string
    names: Array<string> = []
    @Type(() => SensorAndChannelSettings)
    sensorAndChannelSettings: Array<SensorAndChannelSettings> = []
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
    userName?: string

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
    userColor?: Color

    @Exclude() // we don't want to persist this
    channelLabel: string = ''
    userName?: string

    viewType: ChannelViewType = ChannelViewType.Control
    @Type(() => Dashboard)
    channelDashboard?: Dashboard

    constructor(defaultColor: Color = '#568af2') {
        this.defaultColor = defaultColor
    }

    get color(): Color {
        return this.userColor != null ? this.userColor : this.defaultColor
    }

    get name(): string {
        return this.userName != null ? this.userName : this.channelLabel
    }
}

export enum ChannelViewType {
    Control = 'Control',
    Dashboard = 'Dashboard',
}

/**
 * 获取ChannelViewType的本地化显示名称
 * @param type ChannelViewType枚举值
 * @returns 本地化的显示名称
 */
export function getChannelViewTypeDisplayName(type: ChannelViewType): string {
    const { t } = i18n.global
    switch (type) {
        case ChannelViewType.Control:
            return t('models.channelViewType.control')
        case ChannelViewType.Dashboard:
            return t('models.channelViewType.dashboard')
        default:
            return String(type)
    }
}
