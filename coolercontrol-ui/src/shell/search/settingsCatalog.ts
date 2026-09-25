// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Every settings row the palette can reach, with the card and group it lives
// under. Hand-authored rather than scraped: the DOM only exists once Settings
// has been visited, and a scrape could carry no synonyms.
//
// settingsCatalog.spec enforces both directions. Every `<UiSettingRow` in
// AppSettings.vue must carry an id, every static id must appear here, and every
// key here must resolve in english. Adding setting #30 without indexing it
// fails CI.
//
// The Custom Theme card is a `v-for` of colour pickers behind a dynamic id, so
// it is indexed once as a page (see pages.ts) rather than eleven times here.
//
// `keywords` are untranslated on purpose. They stay out of the locale files, so
// the 12-locale parity check never sees them, and the docs and forums they echo
// are english anyway.

export interface SettingsEntry {
    /** Matches the `id` on the row in AppSettings.vue. */
    id: string
    /** Enclosing UiSettingsCard title. */
    cardKey: string
    /** Enclosing UiSettingGroup title, when the card has groups. */
    groupKey?: string
    labelKey: string
    keywords?: readonly string[]
    /** Only rendered in the desktop app. */
    qtOnly?: boolean
}

const GENERAL = 'layout.settings.general'
const APPEARANCE = 'layout.settings.appearance'
const DAEMON = 'views.daemon.title'
const DESKTOP = 'layout.settings.desktop'

const STARTUP = 'layout.settings.groups.startup'
const PERFORMANCE = 'layout.settings.groups.performance'
const DEVICES = 'layout.settings.groups.devices'
const LIQUIDCTL = 'layout.settings.groups.liquidctl'
const LOGGING = 'layout.settings.groups.logging'

export const SETTINGS_ENTRIES: readonly SettingsEntry[] = Object.freeze([
    {
        id: 'setting-introduction',
        cardKey: GENERAL,
        labelKey: 'layout.settings.introduction',
        keywords: ['tour', 'onboarding', 'welcome', 'guide', 'getting started'],
    },
    {
        id: 'setting-shortcuts',
        cardKey: GENERAL,
        labelKey: 'views.shortcuts.shortcuts',
        keywords: ['keybindings', 'hotkeys', 'keys'],
    },
    {
        id: 'setting-startup-page',
        cardKey: GENERAL,
        labelKey: 'layout.settings.startupPage',
        keywords: ['home page', 'landing', 'default page', 'first page'],
    },
    {
        id: 'setting-language',
        cardKey: GENERAL,
        labelKey: 'layout.settings.language',
        keywords: ['locale', 'translation', 'i18n'],
    },
    {
        id: 'setting-full-screen',
        cardKey: GENERAL,
        labelKey: 'layout.settings.fullScreen',
        keywords: ['fullscreen', 'maximise', 'maximize'],
    },
    {
        id: 'setting-theme-style',
        cardKey: APPEARANCE,
        labelKey: 'layout.settings.themeStyle',
        keywords: ['dark mode', 'light mode', 'colors', 'colours', 'appearance', 'contrast'],
    },
    {
        id: 'setting-dashboard-line-size',
        cardKey: APPEARANCE,
        labelKey: 'layout.settings.dashboardLineSize',
        keywords: ['chart', 'graph', 'thickness', 'stroke', 'width'],
    },
    {
        id: 'setting-time-format',
        cardKey: APPEARANCE,
        labelKey: 'layout.settings.timeFormat',
        keywords: ['clock', '12 hour', '24 hour', 'am pm'],
    },
    {
        id: 'setting-frequency-precision',
        cardKey: APPEARANCE,
        labelKey: 'layout.settings.frequencyPrecision',
        keywords: ['mhz', 'ghz', 'clock speed', 'decimals', 'rounding'],
    },
    {
        id: 'setting-eye-candy',
        cardKey: APPEARANCE,
        labelKey: 'layout.settings.eyeCandy',
        keywords: ['animation', 'effects', 'transitions', 'motion'],
    },
    {
        id: 'setting-rail-to-collapse',
        cardKey: APPEARANCE,
        labelKey: 'layout.settings.railToCollapse',
        keywords: ['sidebar', 'menu', 'panel', 'navigation'],
    },
    {
        id: 'setting-interface-font',
        cardKey: APPEARANCE,
        labelKey: 'layout.settings.interfaceFont',
        keywords: ['typeface', 'text size', 'typography'],
    },
    {
        id: 'setting-apply-on-startup',
        cardKey: DAEMON,
        groupKey: STARTUP,
        labelKey: 'layout.settings.applySettingsOnStartup',
        keywords: ['boot', 'restore', 'reapply', 'on boot'],
    },
    {
        id: 'setting-device-startup-delay',
        cardKey: DAEMON,
        groupKey: STARTUP,
        labelKey: 'layout.settings.deviceDelayAtStartup',
        keywords: ['boot', 'wait', 'delay', 'seconds'],
    },
    {
        id: 'setting-polling-rate',
        cardKey: DAEMON,
        groupKey: PERFORMANCE,
        labelKey: 'layout.settings.pollingRate',
        keywords: ['poll rate', 'interval', 'refresh', 'update rate', 'hz', 'sample'],
    },
    {
        id: 'setting-compress-api-payload',
        cardKey: DAEMON,
        groupKey: PERFORMANCE,
        labelKey: 'layout.settings.compressApiPayload',
        keywords: ['gzip', 'compression', 'bandwidth', 'network'],
    },
    {
        id: 'setting-sensors-auto-detect',
        cardKey: DAEMON,
        groupKey: DEVICES,
        labelKey: 'layout.settings.sensorsAutoDetect',
        keywords: ['detection', 'discover', 'scan', 'hwmon'],
    },
    {
        id: 'setting-sensors-config',
        cardKey: DAEMON,
        groupKey: DEVICES,
        labelKey: 'layout.settings.sensorsConfig',
        keywords: ['sensors.conf', 'lm-sensors', 'labels', 'ignore'],
    },
    {
        id: 'setting-device-listener',
        cardKey: DAEMON,
        groupKey: DEVICES,
        labelKey: 'layout.settings.deviceListener',
        keywords: ['hotplug', 'udev', 'plug', 'unplug'],
    },
    {
        id: 'setting-drive-power-state',
        cardKey: DAEMON,
        groupKey: DEVICES,
        labelKey: 'layout.settings.drivePowerState',
        keywords: ['disk', 'hdd', 'ssd', 'spin down', 'standby', 'sleep'],
    },
    {
        id: 'setting-liquidctl-integration',
        cardKey: DAEMON,
        groupKey: LIQUIDCTL,
        labelKey: 'layout.settings.liquidctlIntegration',
        keywords: ['aio', 'usb', 'liqctld'],
    },
    {
        id: 'setting-liquidctl-device-init',
        cardKey: DAEMON,
        groupKey: LIQUIDCTL,
        labelKey: 'layout.settings.liquidctlDeviceInit',
        keywords: ['initialize', 'initialise', 'aio', 'usb'],
    },
    {
        id: 'setting-hide-duplicate-devices',
        cardKey: DAEMON,
        groupKey: LIQUIDCTL,
        labelKey: 'layout.settings.hideDuplicateDevices',
        keywords: ['duplicate', 'hwmon', 'twice', 'double'],
    },
    {
        id: 'setting-debug-logging',
        cardKey: DAEMON,
        groupKey: LOGGING,
        labelKey: 'layout.settings.debugLogging',
        keywords: ['debug', 'logs', 'log level', 'journal', 'journalctl', 'support', 'verbose'],
    },
    {
        id: 'setting-start-in-tray',
        cardKey: DESKTOP,
        labelKey: 'layout.settings.startInTray',
        keywords: ['minimised', 'minimized', 'background', 'system tray'],
        qtOnly: true,
    },
    {
        id: 'setting-close-to-tray',
        cardKey: DESKTOP,
        labelKey: 'layout.settings.closeToTray',
        keywords: ['minimise', 'minimize', 'background', 'system tray', 'x button'],
        qtOnly: true,
    },
    {
        id: 'setting-zoom',
        cardKey: DESKTOP,
        labelKey: 'layout.settings.zoom',
        keywords: ['scale', 'dpi', 'text size', 'magnify'],
        qtOnly: true,
    },
    {
        id: 'setting-desktop-startup-delay',
        cardKey: DESKTOP,
        labelKey: 'layout.settings.desktopStartupDelay',
        keywords: ['boot', 'wait', 'delay', 'autostart'],
        qtOnly: true,
    },
])
