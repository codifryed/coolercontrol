// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { Color } from '@/models/Device'
import { Exclude, Type } from 'class-transformer'
import type { UID } from '@/models/Device'
import { Dashboard } from '@/models/Dashboard.ts'
import i18n from '@/i18n'
import { installedTheme, type ThemeTokens } from '@/shell/themes.ts'

export class TagSettings {
    name: string
    color: Color

    constructor(name: string, color: Color) {
        this.name = name
        this.color = color
    }
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
 * The compiled themes, in picker order. Custom is listed separately because it
 * sits last, after the installed themes.
 */
export const BUILT_IN_THEME_MODES: ThemeMode[] = [
    ThemeMode.SYSTEM,
    ThemeMode.DARK,
    ThemeMode.LIGHT,
    ThemeMode.HIGH_CONTRAST_DARK,
    ThemeMode.HIGH_CONTRAST_LIGHT,
]

/**
 * The display name for a theme selection. Built-in modes are translated;
 * installed themes are proper nouns and keep their own name.
 */
export function getThemeModeDisplayName(mode: string): string {
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
            return installedTheme(mode)?.name ?? String(mode)
    }
}

/**
 * A custom theme carries the same tokens an installed theme does, so both are
 * applied through the one variable map in `shell/themes.ts`.
 */
export type CustomThemeSettings = Record<keyof ThemeTokens, Color>

export const defaultCustomTheme: CustomThemeSettings = {
    // default dark-theme
    accent: '77 140 255', //'#4d8cff'
    accentGradientTo: '255 33 255', //'#ff21ff'
    bgOne: '27 30 35', //'#1b1e23'
    bgTwo: '44 49 60', //'#2c313c'
    borderOne: '138 149 170 0.25', //'#8a95aa40'
    textColor: '220 225 236', //'#dce1ec'
    textColorSecondary: '138 149 170', //'#8a95aa'
    success: '0 255 127', //'#00ff7f'
    warning: '241 250 140', //'#f1fa8c'
    error: '255 85 85', //'#ff5555'
    info: '86 138 242', //'#568af2'
}

/**
 * Which fonts the interface uses. `System` hands both the label and the value
 * role back to the user's own fonts, which is the only font preference a Qt app
 * user can express: QtWebEngine has no override UI.
 */
export enum InterfaceFont {
    BUNDLED = 'bundled',
    SYSTEM = 'system',
}

export function getInterfaceFontDisplayName(font: InterfaceFont): string {
    const { t } = i18n.global
    return font === InterfaceFont.SYSTEM
        ? t('models.interfaceFont.system')
        : t('models.interfaceFont.bundled')
}

export enum ChannelViewType {
    Control = 'Control',
    Dashboard = 'Dashboard',
}

export enum StartupPage {
    AppInfo = 'app-info',
    HomeDashboard = 'dashboards',
    Controls = 'system-controls',
}

/**
 * Returns the localized display name for a StartupPage value.
 */
export function getStartupPageDisplayName(page: StartupPage): string {
    const { t } = i18n.global
    switch (page) {
        case StartupPage.AppInfo:
            return t('models.startupPage.appInfo')
        case StartupPage.HomeDashboard:
            return t('models.startupPage.homeDashboard')
        case StartupPage.Controls:
            return t('models.startupPage.controls')
        default:
            return String(page)
    }
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
    tags: Array<string> = []
    // The profile last seen driving this channel. A fixed speed or an unmanaged
    // channel carries no profile in the daemon's setting, so this is the only
    // record of what to offer back when the channel returns to one.
    lastProfileUID?: UID

    constructor(defaultColor: Color = '#568af2') {
        this.defaultColor = defaultColor
    }

    get color(): Color {
        return this.userColor != null ? this.userColor : this.defaultColor
    }

    get name(): string {
        // channelLabel carries the resolved display label (user overrides
        // included); userName is round-trip-only and slated for removal.
        return this.channelLabel
    }
}

export class DeviceUISettingsDTO {
    userName?: string
    userColor?: Color
    names: Array<string> = []
    @Type(() => SensorAndChannelSettings)
    sensorAndChannelSettings: Array<SensorAndChannelSettings> = []
}

export interface MenuOrderIds {
    id: string
    children: Array<string>
}

export type AllDeviceSettings = Map<UID, DeviceUISettings>

/**
 * A Device's Settings
 */
export class DeviceUISettings {
    displayName: string = ''
    userName?: string
    // An optional color for the device, default is the theme's text color:
    userColor?: Color

    /**
     * A Map of Sensor and Channel Names to associated Settings.
     */
    readonly sensorsAndChannels: Map<string, SensorAndChannelSettings> = new Map()

    get name(): string {
        // displayName carries the resolved display name (user overrides
        // included); userName is round-trip-only and slated for removal.
        return this.displayName
    }
}

// Bump when the tour changes enough that everyone should see it again. 1 was
// the original tour; 2 reworked it for the new shell; 3 reran it for the shell's
// rail. Later removals of steps do not bump it: nobody needs to sit through the
// tour again to be shown less.
export const ONBOARDING_TOUR_VERSION = 3

/**
 * A DTO Class to hold all the UI settings to be persisted by the daemon.
 * The Class-Transformer has issues with Maps, so we have to use Arrays to
 * store that data and do the transformation.
 */
// Which corner an overlay sits in: a graph editor's points table, or the chart stats panel.
export type OverlayPosition = 'top-left' | 'bottom-right'

export type StatsLegendScope = 'window' | 'since-start'

export class UISettingsDTO {
    devices?: Array<UID> = []

    @Type(() => DeviceUISettingsDTO)
    deviceSettings?: Array<DeviceUISettingsDTO> = []

    @Type(() => Dashboard)
    dashboards: Array<Dashboard> = []
    homeDashboard?: UID
    themeMode: string = ThemeMode.SYSTEM
    chartLineScale: number = 1.5
    time24: boolean = false
    menuOrder: Array<MenuOrderIds> = []
    // Library folder names, id -> name. The folders themselves are
    // menuOrder entries; only their names need a home of their own.
    libraryFolderNames: Array<[string, string]> = []
    expandedMenuIds: Array<string> | undefined
    pinnedIds: Array<string> = []
    collapsedMainMenu: boolean = false
    // Legacy name: in the old shell this hid the rail's collapse icon and put the
    // toggle on the rail's empty space instead. The header button now stays either
    // way, so this only adds the empty space as a second target. Kept under the old
    // name so anyone who had it on keeps it on.
    hideMenuCollapseIcon: boolean = false
    mainMenuWidthRem: number = 24
    frequencyPrecision: number = 1
    customTheme: CustomThemeSettings = { ...defaultCustomTheme }
    entityColors: Array<[string, string]> = []
    eyeCandy: boolean = false
    // Per profile, since where the points table is out of the way depends on the curve's shape.
    pointsOverlayTablePositions: Array<[UID, OverlayPosition]> = []
    // The stats panel on individual channel charts. Global rather than per channel, so
    // moving or hiding it never rebuilds the chart.
    sensorStatsPanelVisible: boolean = true
    sensorStatsPanelPosition: OverlayPosition = 'top-left'
    // What the dashboard stats legend's min/max/avg cover. Global for the same reason.
    statsLegendScope: StatsLegendScope = 'window'
    interfaceFont: InterfaceFont = InterfaceFont.BUNDLED
    // Undefined means the user has never chosen, and `system` means follow the
    // browser locale. Never a resolved code for a non-choice: that is what let
    // a system locale masquerade as a deliberate pick when this lived only in
    // localStorage.
    language?: string
    // The tour version the user has completed; below ONBOARDING_TOUR_VERSION
    // means it runs again. Legacy configs hold a boolean here, which coerces
    // to 0 (dismissed) or 1 (never run) and so replays the reworked tour once.
    showOnboarding: number = 0
    tagNames: Array<string> = []
    tagColors: Array<string> = []
    cpuStressBackend: 'stress_ng' | 'built_in' = 'built_in'
    gpuStressBackend: 'stress_ng' | 'built_in' = 'built_in'
    ramStressBackend: 'stress_ng' | 'built_in' = 'built_in'
    driveStressBackend: 'stress_ng' | 'built_in' = 'built_in'
    startupPage: StartupPage = StartupPage.AppInfo
}
