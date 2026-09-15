// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { defineStore } from 'pinia'
import i18n from '@/i18n'
import { cacheLanguage, languageSetting, resolveLanguage, SYSTEM_LANGUAGE } from '@/i18n/locale.ts'
import { Function, FunctionsDTO, Profile, ProfilesDTO } from '@/models/Profile'
import type { Ref } from 'vue'
import { computed, reactive, inject, ref, toRaw, watch, watchEffect } from 'vue'
import {
    type AllDeviceSettings,
    CustomThemeSettings,
    defaultCustomTheme,
    DeviceUISettings,
    DeviceUISettingsDTO,
    InterfaceFont,
    MenuOrderIds,
    ONBOARDING_TOUR_VERSION,
    SensorAndChannelSettings,
    StartupPage,
    TagSettings,
    type TablePosition,
    ThemeMode,
    UISettingsDTO,
} from '@/models/UISettings'
import {
    hexToTriplet,
    installedTheme,
    parseSystemPalette,
    surfaceTintFor,
    SYSTEM_THEME_ID,
    type SystemPalette,
    THEME_CSS_VAR_NAMES,
    THEME_TOKEN_KEYS,
    THEME_TOKEN_VARS,
    themeCssVars,
} from '@/shell/themes.ts'
import type { Color, UID } from '@/models/Device'
import { Device } from '@/models/Device'
import setDefaultSensorAndChannelColors from '@/stores/DeviceColorCreator'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { buildPinnedSensors } from '@/shell/qtPinnedSensors.ts'
import { channelRoute } from '@/shell/channelRoute.ts'
import router from '@/router'
import type { AllDaemonDeviceSettings } from '@/models/DaemonSettings'
import type { NameOverrides } from '@/models/NameOverrides'
import {
    DaemonDeviceSettings,
    DeviceSettingReadDTO,
    DeviceSettingWriteLcdDTO,
    DeviceSettingWriteLightingDTO,
    DeviceSettingWriteManualDTO,
    DeviceSettingWriteProfileDTO,
    DeviceSettingWritePWMModeDTO,
} from '@/models/DaemonSettings'
import { useToast } from '@/shell/toast'
import { CoolerControlDeviceSettingsDTO, CoolerControlSettingsDTO } from '@/models/CCSettings'
import { ErrorResponse } from '@/models/ErrorResponse'
import { CustomSensor } from '@/models/CustomSensor'
import { CreateModeDTO, Mode, ModeOrderDTO, UpdateModeDTO } from '@/models/Mode.ts'
import { PowerProfileModesDTO, SystemEventDTO } from '@/models/PowerProfile.ts'
import { Dashboard } from '@/models/Dashboard.ts'
import { Emitter, EventType } from 'mitt'
import _ from 'lodash'
import { Alert, AlertLog, AlertState, alertIsSilencedAt } from '@/models/Alert.ts'
import {
    ChannelVerdictRef,
    DeviceHealthDTO,
    UnreachableDelta,
    UnreachableRef,
    FailsafeDelta,
    failsafeKey,
    FailsafeRef,
    HealthState,
    SourceDelta,
    sourceKey,
    SourceRef,
    SystemFinding,
} from '@/models/DeviceHealth.ts'
import { useI18n } from 'vue-i18n'

export const useSettingsStore = defineStore('settings', () => {
    const toast = useToast()

    // The daemon's sparse name overrides document, the source of truth for
    // user-defined display names. Loaded once at startup; updated locally
    // on rename so dialogs can pre-fill without a re-fetch.
    const nameOverrides: Ref<NameOverrides> = ref({ devices: {} })
    const { t } = useI18n()

    const deviceStore = useDeviceStore() // using another store internally in this way seems ok, as long as we don't have a circular dependency
    const colorStore = useThemeColorsStore()
    const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
    const predefinedColorOptions: Ref<Array<string>> = ref([
        '#FFFFFF',
        '#000000',
        '#FF0000',
        '#FFFF00',
        '#00FF00',
        '#00FFFF',
        '#0000FF',
        '#FF00FF',
    ])

    const functions: Ref<Array<Function>> = ref([])

    const profiles: Ref<Array<Profile>> = ref([])

    const modes: Ref<Array<Mode>> = ref([])

    const modeActiveCurrent: Ref<UID | undefined> = ref()
    // System power profile integration. `powerProfilesAvailable` stays empty when no power
    // profile daemon is reachable, which is how the UI knows to hide the mapping entirely.
    const powerProfilesAvailable: Ref<Array<string>> = ref([])
    const powerProfileActive: Ref<string | undefined> = ref()
    const powerProfileModes: Ref<Record<string, UID>> = ref({})
    const modeActivePrevious: Ref<UID | undefined> = ref()

    const modeInEdit: Ref<UID | undefined> = ref()

    const alerts: Ref<Array<Alert>> = ref([])
    const alertLogs: Ref<Array<AlertLog>> = ref([])
    const alertsActive: Ref<Array<UID>> = ref([])
    // Error needs a persistent indicator of its own rather than going dark.
    const alertsError: Ref<Array<UID>> = ref([])

    // A silence expires by the clock passing its timestamp, which is not something a
    // computed can depend on, so the badge below would stay cleared for a still-firing
    // alert. This ref is that missing dependency. It wakes once per pending expiry
    // rather than polling, so nothing runs while nothing is silenced.
    const silenceClock: Ref<number> = ref(Date.now())
    let silenceTimer: ReturnType<typeof setTimeout> | undefined
    watchEffect(() => {
        clearTimeout(silenceTimer)
        // Read so each tick re-arms the next one, for silences that expire in sequence.
        void silenceClock.value
        const now = Date.now()
        let soonest = Number.POSITIVE_INFINITY
        for (const alert of alerts.value) {
            if (alert.silenced_until == null) continue
            const until = new Date(alert.silenced_until).getTime()
            if (until > now && until < soonest) soonest = until
        }
        if (soonest === Number.POSITIVE_INFINITY) return
        silenceTimer = setTimeout(() => (silenceClock.value = Date.now()), soonest - now + 250)
    })

    // The Qt tray badge mirrors the UI's alert state. Silencing/disabling happen in
    // the UI, and the daemon emits nothing on the wire for a steadily-Active alert
    // that becomes silenced or disabled, so push the derived state to Qt over IPC
    // instead of polling. `enabled` + not-silenced gate out muted alerts.
    const clearAlertAttention = (alertUID: UID): void => {
        for (const set of [alertsActive, alertsError]) {
            for (let index = set.value.length - 1; index >= 0; index--) {
                if (set.value[index] === alertUID) set.value.splice(index, 1)
            }
        }
    }
    const alertNeedsAttention = (alert: Alert, now: number): boolean =>
        alert.enabled &&
        (alertsActive.value.includes(alert.uid) || alertsError.value.includes(alert.uid)) &&
        !alertIsSilencedAt(alert, now)
    const alertsNeedingAttention = computed((): Array<Alert> => {
        const now = silenceClock.value
        return alerts.value.filter((alert) => alertNeedsAttention(alert, now))
    })
    const anyActiveUnsilencedAlert = computed(
        (): boolean => alertsNeedingAttention.value.length > 0,
    )
    const pushTrayAlertState = (): void => {
        if (!deviceStore.isQtApp()) return
        // @ts-ignore - window.ipc is the QWebChannel bridge, present only in the Qt app.
        window.ipc?.setAlertsActive?.(anyActiveUnsilencedAlert.value)
    }
    watch(anyActiveUnsilencedAlert, () => pushTrayAlertState())

    const healthFailsafe: Ref<Array<FailsafeRef>> = ref([])
    const healthUnreachable: Ref<Array<UnreachableRef>> = ref([])
    const healthMissing: Ref<Array<SourceRef>> = ref([])
    const healthStaleSource: Ref<Array<SourceRef>> = ref([])
    // Permanent hardware facts, kept apart from the fault lists above so a
    // capability is never rendered as something that might clear on its own.
    const healthChannelCapabilities: Ref<Array<ChannelVerdictRef>> = ref([])
    const healthFirmwareOverrides: Ref<Array<ChannelVerdictRef>> = ref([])
    const healthSystemFindings: Ref<Array<SystemFinding>> = ref([])

    /**
     * The daemon's verdict for one channel, or undefined when it reported none.
     * Controllable channels are absent by design: the daemon only publishes
     * what it cannot drive.
     */
    function channelVerdict(deviceUID: UID, channelName: string): ChannelVerdictRef | undefined {
        const matches = (ref: ChannelVerdictRef): boolean =>
            ref.device_uid === deviceUID && ref.channel_name === channelName
        return (
            healthFirmwareOverrides.value.find(matches) ??
            healthChannelCapabilities.value.find(matches)
        )
    }

    const allUIDeviceSettings: Ref<AllDeviceSettings> = ref(new Map<UID, DeviceUISettings>())

    const allDaemonDeviceSettings: Ref<AllDaemonDeviceSettings> = ref(
        new Map<UID, DaemonDeviceSettings>(),
    )

    const ccSettings: Ref<CoolerControlSettingsDTO> = ref(new CoolerControlSettingsDTO())

    const ccDeviceSettings: Ref<Map<UID, CoolerControlDeviceSettingsDTO>> = ref(
        new Map<UID, CoolerControlDeviceSettingsDTO>(),
    )
    const ccBlacklistedDevices: Ref<Map<UID, CoolerControlDeviceSettingsDTO>> = ref(
        new Map<UID, CoolerControlDeviceSettingsDTO>(),
    )

    const thinkPadFanControlEnabled: Ref<boolean> = ref(false)

    const dashboards: Array<Dashboard> = reactive([...Dashboard.default()])
    const homeDashboard: Ref<UID | undefined> = ref()
    const chartLineScale: Ref<number> = ref(1.5)
    const startInSystemTray: Ref<boolean> = ref(false)
    const closeToSystemTray: Ref<boolean> = ref(false)
    const desktopStartupDelay: Ref<number> = ref(0)
    const themeMode: Ref<string> = ref(ThemeMode.SYSTEM)
    // The desktop's own colors, pushed by the Qt app. Null in a browser and on any
    // desktop that publishes none, and read only while themeMode is System.
    const systemPalette: Ref<SystemPalette | null> = ref(null)
    const uiScale: Ref<number> = ref(100)
    const time24: Ref<boolean> = ref(false)
    const menuOrder: Ref<Array<MenuOrderIds>> = ref([])
    // Replaced wholesale, never mutated in place: like menuOrder, the save
    // watcher below only fires on a new array.
    const libraryFolderNames: Ref<Array<[string, string]>> = ref([])
    const expandedMenuIds: Ref<Array<string> | undefined> = ref()
    const pinnedIds: Ref<Array<string>> = ref([])

    // The tray lists the pinned sensors, and Qt fetches their readings itself because
    // the renderer is gone once the window is in the tray. Only identity and label
    // travel over IPC; Qt caches them so the list survives a discarded page.
    const pushTrayPinnedSensors = (): void => {
        if (!deviceStore.isQtApp()) return
        const sensors = buildPinnedSensors(
            deviceStore.allDevices(),
            pinnedIds.value,
            (deviceUID, channelName) =>
                allUIDeviceSettings.value.get(deviceUID)?.sensorsAndChannels.get(channelName)
                    ?.name ?? channelName,
            // Generated default colours arrive as CSS rgb(), user-set ones as hex. Qt's
            // QColor parses only hex, so normalise here rather than teaching C++ to read
            // CSS; an unparsed colour silently renders no swatch at all.
            (deviceUID, channelName) =>
                colorStore.rgbToHex(
                    allUIDeviceSettings.value.get(deviceUID)?.sensorsAndChannels.get(channelName)
                        ?.color ?? '',
                ),
            (deviceUID, channelName) =>
                router.resolve(channelRoute(deviceStore.allDevices(), deviceUID, channelName)).href,
        )
        // @ts-ignore - window.ipc is the QWebChannel bridge, present only in the Qt app.
        window.ipc?.setPinnedSensors?.(JSON.stringify(sensors))
    }
    watch(pinnedIds, () => pushTrayPinnedSensors(), { deep: true })
    const collapsedMainMenu: Ref<boolean> = ref(false)
    // Whether the rail's empty space also toggles the menu panel. See UISettingsDTO
    // for why the name talks about an icon it no longer hides.
    const hideMenuCollapseIcon: Ref<boolean> = ref(false)
    const mainMenuWidthRem: Ref<number> = ref(24)
    const frequencyPrecision: Ref<number> = ref(1)
    const customTheme: CustomThemeSettings = reactive({ ...defaultCustomTheme })
    const entityColors: Ref<Array<[string, string]>> = ref([])
    const eyeCandy: Ref<boolean> = ref(false)
    // The corner each profile's points overlay table was last moved to, by profile UID.
    const pointsOverlayTablePositions: Ref<Array<[UID, TablePosition]>> = ref([])
    const pointsTablePosition = (profileUID: UID): TablePosition =>
        pointsOverlayTablePositions.value.find(([uid]) => uid === profileUID)?.[1] ?? 'bottom-right'
    // Replaces the array rather than mutating it: the settings saver watches this ref without
    // deep: true, so only a new value reaches it.
    const setPointsTablePosition = (profileUID: UID, position: TablePosition): void => {
        pointsOverlayTablePositions.value = [
            ...pointsOverlayTablePositions.value.filter(([uid]) => uid !== profileUID),
            [profileUID, position],
        ]
    }
    const interfaceFont: Ref<InterfaceFont> = ref(InterfaceFont.BUNDLED)
    // The chosen language, not the resolved one: `system` follows the browser.
    const language: Ref<string> = ref(SYSTEM_LANGUAGE)
    // What the daemon had, held between loading the settings and adopting it
    // below, which cannot happen until the save watcher is running.
    let persistedLanguage: string | undefined
    // Persisted as the tour version the user has finished. Callers only ask the
    // yes/no question, so they read the computed below and call
    // completeOnboarding() rather than writing a flag.
    const onboardingSeenVersion: Ref<number> = ref(0)
    const showOnboarding = computed(() => onboardingSeenVersion.value < ONBOARDING_TOUR_VERSION)
    const completeOnboarding = (): void => {
        onboardingSeenVersion.value = ONBOARDING_TOUR_VERSION
    }
    const cpuStressBackend: Ref<'stress_ng' | 'built_in'> = ref('stress_ng')
    const gpuStressBackend: Ref<'stress_ng' | 'built_in'> = ref('built_in')
    const ramStressBackend: Ref<'stress_ng' | 'built_in'> = ref('stress_ng')
    const driveStressBackend: Ref<'stress_ng' | 'built_in'> = ref('built_in')
    const startupPage: Ref<StartupPage> = ref(StartupPage.AppInfo)
    const tags: Ref<Map<string, TagSettings>> = ref(new Map<string, TagSettings>())

    async function initializeSettings(allDevicesIter: IterableIterator<Device>): Promise<void> {
        await loadCCSettings()

        // set defaults for all devices:
        const allDevices = [...allDevicesIter]
        for (const device of allDevices) {
            const deviceSettings = new DeviceUISettings()
            // Prepare all base settings:
            for (const temp of device.status.temps) {
                deviceSettings.sensorsAndChannels.set(temp.name, new SensorAndChannelSettings())
            }
            for (const channel of device.status.channels) {
                if (channel.name.toLowerCase().includes('load')) {
                    deviceSettings.sensorsAndChannels.set(
                        channel.name,
                        new SensorAndChannelSettings(),
                    )
                } else if (channel.name.toLowerCase().includes('freq')) {
                    deviceSettings.sensorsAndChannels.set(
                        channel.name,
                        new SensorAndChannelSettings(),
                    )
                } else if (channel.name.toLowerCase().includes('power')) {
                    deviceSettings.sensorsAndChannels.set(
                        channel.name,
                        new SensorAndChannelSettings(),
                    )
                }
            }
            if (device.info != null) {
                if (device.info.thinkpad_fan_control != null) {
                    thinkPadFanControlEnabled.value = device.info.thinkpad_fan_control
                }
                // The Monitoring panel lists sensors from device.info, so every
                // info temp/channel needs a settings entry. Otherwise an info-only
                // sensor (e.g. a plugin temp not currently in status) has no color
                // and blanks the panel.
                for (const tempName of device.info.temps.keys()) {
                    if (!deviceSettings.sensorsAndChannels.has(tempName)) {
                        deviceSettings.sensorsAndChannels.set(
                            tempName,
                            new SensorAndChannelSettings(),
                        )
                    }
                }
                for (const [channelName, channelInfo] of device.info.channels.entries()) {
                    if (channelInfo.speed_options != null) {
                        deviceSettings.sensorsAndChannels.set(
                            channelName,
                            new SensorAndChannelSettings(),
                        )
                    } else if (channelInfo.lighting_modes.length > 0) {
                        deviceSettings.sensorsAndChannels.set(
                            channelName,
                            new SensorAndChannelSettings(),
                        )
                    } else if (channelInfo.lcd_modes.length > 0) {
                        deviceSettings.sensorsAndChannels.set(
                            channelName,
                            new SensorAndChannelSettings(),
                        )
                    } else if (!deviceSettings.sensorsAndChannels.has(channelName)) {
                        deviceSettings.sensorsAndChannels.set(
                            channelName,
                            new SensorAndChannelSettings(),
                        )
                    }
                }
            }
            allUIDeviceSettings.value.set(device.uid, deviceSettings)
        }

        // load settings from persisted settings, overwriting those that are set
        const uiSettings = await deviceStore.daemonClient.loadUISettings()
        if (uiSettings.dashboards.length > 0) {
            dashboards.length = 0
            dashboards.push(...uiSettings.dashboards)
        }
        homeDashboard.value = uiSettings.homeDashboard
        if (homeDashboard.value == null) {
            // set home dashboard to first dashboard by default
            homeDashboard.value = dashboards[0].uid
        }
        chartLineScale.value = uiSettings.chartLineScale
        if (deviceStore.isQtApp()) {
            // @ts-ignore
            const ipc = window.ipc
            try {
                startInSystemTray.value = await ipc.getStartInTray()
                desktopStartupDelay.value = await ipc.getStartupDelay()
                closeToSystemTray.value = await ipc.getCloseToTray()
                uiScale.value = (await ipc.getZoomFactor()) * 100
                // Optional throughout: a Qt app older than this feature has neither
                // the getter nor the signal, and must not fail the whole block.
                systemPalette.value = parseSystemPalette((await ipc.getSystemPalette?.()) ?? '')
                // The accent and the light/dark preference can both change while
                // the app runs, so follow them rather than reading once.
                ipc.systemPaletteChanged?.connect((paletteJson: string) => {
                    systemPalette.value = parseSystemPalette(paletteJson)
                    applyThemeMode()
                })
            } catch (err: any) {
                console.error('Failed to get desktop setting: ', err)
            }
        }
        themeMode.value = uiSettings.themeMode
        applyThemeMode()
        time24.value = uiSettings.time24
        menuOrder.value = uiSettings.menuOrder
        libraryFolderNames.value = uiSettings.libraryFolderNames ?? []
        expandedMenuIds.value = uiSettings.expandedMenuIds
        pinnedIds.value = uiSettings.pinnedIds
        collapsedMainMenu.value = uiSettings.collapsedMainMenu
        hideMenuCollapseIcon.value = uiSettings.hideMenuCollapseIcon ?? false
        mainMenuWidthRem.value = uiSettings.mainMenuWidthRem
        frequencyPrecision.value = uiSettings.frequencyPrecision
        // Settings saved before the status colors existed have only the first
        // six keys; the rest keep their defaults.
        for (const key of THEME_TOKEN_KEYS) {
            const saved = uiSettings.customTheme?.[key]
            if (saved != null) customTheme[key] = saved
        }
        entityColors.value = uiSettings.entityColors
        eyeCandy.value = uiSettings.eyeCandy
        pointsOverlayTablePositions.value = uiSettings.pointsOverlayTablePositions ?? []
        interfaceFont.value = uiSettings.interfaceFont ?? InterfaceFont.BUNDLED
        applyInterfaceFont()
        persistedLanguage = uiSettings.language
        // Legacy configs stored a boolean here: false once the old tour was
        // dismissed, true when it had never run. Both coerce below the current
        // version, so either way the reworked tour plays once.
        onboardingSeenVersion.value = Number(uiSettings.showOnboarding) || 0
        cpuStressBackend.value = uiSettings.cpuStressBackend ?? 'stress_ng'
        gpuStressBackend.value = uiSettings.gpuStressBackend ?? 'built_in'
        ramStressBackend.value = uiSettings.ramStressBackend ?? 'stress_ng'
        driveStressBackend.value = uiSettings.driveStressBackend ?? 'built_in'
        startupPage.value = uiSettings.startupPage ?? StartupPage.AppInfo
        tags.value.clear()
        if (uiSettings.tagNames.length === uiSettings.tagColors.length) {
            for (const [i, name] of uiSettings.tagNames.entries()) {
                tags.value.set(name, new TagSettings(name, uiSettings.tagColors[i]))
            }
        }
        // const layout = useLayout()
        // layout.setScale(uiSettings.uiScale)
        if (
            uiSettings.devices != null &&
            uiSettings.deviceSettings != null &&
            uiSettings.devices.length === uiSettings.deviceSettings.length
        ) {
            for (const [i1, uid] of uiSettings.devices.entries()) {
                const deviceSettingsDto = uiSettings.deviceSettings[i1]
                //  overwrite the defaults, but don't delete any new device/channel defaults
                const deviceSettings = allUIDeviceSettings.value.has(uid)
                    ? allUIDeviceSettings.value.get(uid)!
                    : new DeviceUISettings()
                deviceSettings.userName = deviceSettingsDto.userName
                deviceSettings.userColor = deviceSettingsDto.userColor
                if (
                    deviceSettingsDto.names.length !==
                    deviceSettingsDto.sensorAndChannelSettings.length
                ) {
                    continue
                }
                const savedSensorsAndChannels = new Map<string, SensorAndChannelSettings>()
                for (const [i2, name] of deviceSettingsDto.names.entries()) {
                    savedSensorsAndChannels.set(
                        name,
                        deviceSettingsDto.sensorAndChannelSettings[i2],
                    )
                }
                // merge the saved settings with the defaults:
                for (const [name, sensorAndChannelSettings] of savedSensorsAndChannels) {
                    if (deviceSettings.sensorsAndChannels.has(name)) {
                        deviceSettings.sensorsAndChannels.set(name, sensorAndChannelSettings)
                    }
                }
                allUIDeviceSettings.value.set(uid, deviceSettings)
            }
        }

        setDefaultSensorAndChannelColors(allDevices, allUIDeviceSettings.value)
        nameOverrides.value = await deviceStore.daemonClient.loadNameOverrides()
        setDisplayNames(allDevices, allUIDeviceSettings.value)
        await loadDaemonDeviceSettings()
        await loadCCAllDeviceSettings()

        await loadAlertsAndLogs()
        await loadDeviceHealth()
        await loadFunctions()
        await loadProfiles()
        await loadModes()
        await getActiveModes()
        await loadPowerProfiles()

        await startWatchingToSaveChanges()
        adoptLanguageSetting()
    }

    async function loadCCSettings(): Promise<void> {
        ccSettings.value = await deviceStore.daemonClient.loadCCSettings()
    }

    function findDevice(deviceUID: UID): Device | undefined {
        for (const device of deviceStore.allDevices()) {
            if (device.uid === deviceUID) return device
        }
        return undefined
    }

    /**
     * The device name shown when no user override is set. Takes the model name
     * if it's available, before the driver name (HWMon especially).
     */
    function detectedDeviceName(device: Device): string {
        if (device.info?.model != null && device.info.model.length > 0) return device.info.model
        const deviceOverrides = nameOverrides.value.devices[device.uid]
        // The daemon serves an active override in place of the device name, so
        // its detected-name hint is all that is left to fall back to.
        if (deviceOverrides?.name != null && deviceOverrides.device_name != null) {
            return deviceOverrides.device_name.split(' (')[0] // Device.nameShort's shortening
        }
        return device.nameShort
    }

    /** The channel label shown when no user override is set. */
    function detectedChannelLabel(device: Device, channelName: string): string | undefined {
        // Same boundary resolution as the device name: an active override is
        // served in place of the detected label, so the hint stands in for it.
        const channelOverrides = nameOverrides.value.devices[device.uid]?.channels?.[channelName]
        const served = (label: string | undefined): string | undefined =>
            channelOverrides?.label != null ? channelOverrides.channel_label : label

        const tempInfo = device.info?.temps.get(channelName)
        if (tempInfo != null) return served(tempInfo.label)
        const channelInfo = device.info?.channels.get(channelName)
        if (channelInfo != null) {
            if (channelInfo.speed_options != null) {
                return served(channelInfo.label) ?? deviceStore.toTitleCase(channelName)
            }
            if (channelInfo.lighting_modes.length > 0) return deviceStore.toTitleCase(channelName)
            if (channelInfo.lcd_modes.length > 0) return channelName.toUpperCase()
            // must be Frequency
            return served(channelInfo.label) ?? deviceStore.toTitleCase(channelName)
        }
        // Load channels are only present in the status.
        if (channelName.toLowerCase().includes('load')) return channelName
        return undefined
    }

    /** The name a device rename field falls back to when it is cleared. */
    function defaultDeviceName(deviceUID: UID): string {
        const device = findDevice(deviceUID)
        return device != null ? detectedDeviceName(device) : deviceUID
    }

    /** The label a channel rename field falls back to when it is cleared. */
    function defaultChannelLabel(deviceUID: UID, channelName: string): string {
        const device = findDevice(deviceUID)
        const label = device != null ? detectedChannelLabel(device, channelName) : undefined
        return label ?? deviceStore.toTitleCase(channelName)
    }

    function setDisplayNames(
        devices: Array<Device>,
        deviceSettings: Map<UID, DeviceUISettings>,
    ): void {
        for (const device of devices) {
            const settings = deviceSettings.get(device.uid)!
            const overrides = nameOverrides.value.devices[device.uid]
            settings.displayName = overrides?.name ?? detectedDeviceName(device)
            // User-defined labels win over every detected label:
            for (const [channelName, channelSettings] of settings.sensorsAndChannels) {
                const detected = detectedChannelLabel(device, channelName)
                if (detected != null) channelSettings.channelLabel = detected
                const label = overrides?.channels?.[channelName]?.label
                if (label != null) channelSettings.channelLabel = label
            }
        }
    }

    /**
     * Persists the user-defined device display name as a daemon name
     * override and updates local display state. An empty name removes the
     * override and falls back to the detected name.
     */
    async function saveDeviceName(deviceUID: UID, newName: string): Promise<boolean> {
        const name = newName.length > 0 ? newName : null
        const result = await deviceStore.daemonClient.saveDeviceNameOverride(deviceUID, name)
        if (result !== true) {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: result.error,
                life: 4000,
            })
            return false
        }
        if (name == null) {
            // Resolved before the override is dropped: it is what stands in for
            // the overridden name the daemon serves.
            const detected = defaultDeviceName(deviceUID)
            delete nameOverrides.value.devices[deviceUID]?.name
            const settings = allUIDeviceSettings.value.get(deviceUID)
            if (settings != null) settings.displayName = detected
            return true
        }
        const deviceOverrides = (nameOverrides.value.devices[deviceUID] ??= {})
        deviceOverrides.name = name
        const deviceSettings = allUIDeviceSettings.value.get(deviceUID)
        if (deviceSettings != null) {
            deviceSettings.displayName = name
        }
        return true
    }

    /**
     * Persists the user-defined channel display label as a daemon name
     * override and updates local display state. An empty name removes the
     * override and falls back to the detected label.
     */
    async function saveChannelName(
        deviceUID: UID,
        channelName: string,
        newName: string,
    ): Promise<boolean> {
        const label = newName.length > 0 ? newName : null
        const result = await deviceStore.daemonClient.saveChannelLabelOverride(
            deviceUID,
            channelName,
            label,
        )
        if (result !== true) {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: result.error,
                life: 4000,
            })
            return false
        }
        const channelSettings = allUIDeviceSettings.value
            .get(deviceUID)
            ?.sensorsAndChannels.get(channelName)
        if (label == null) {
            // Resolved before the override is dropped: it is what stands in for
            // the overridden label the daemon serves.
            const detected = defaultChannelLabel(deviceUID, channelName)
            delete nameOverrides.value.devices[deviceUID]?.channels?.[channelName]?.label
            if (channelSettings != null) channelSettings.channelLabel = detected
            return true
        }
        const deviceOverrides = (nameOverrides.value.devices[deviceUID] ??= {})
        const channels = (deviceOverrides.channels ??= {})
        const channel = (channels[channelName] ??= {})
        if (channel.label == null && channelSettings != null) {
            // First override for this channel: the current display label is
            // the detected one; keep it locally as the reset hint, mirroring
            // the daemon-stamped channel_label.
            channel.channel_label ??= channelSettings.channelLabel
        }
        channel.label = label
        if (channelSettings != null) {
            channelSettings.channelLabel = label
        }
        return true
    }

    async function loadDaemonDeviceSettings(
        deviceUID: string | undefined = undefined,
    ): Promise<void> {
        // allDevices() is used to handle cases where a device may be hidden and no longer available
        for (const device of deviceStore.allDevices()) {
            // we could load these in parallel, but it's anyway really fast
            if (deviceUID != null && device.uid !== deviceUID) {
                continue
            }
            const deviceSettingsDTO = await deviceStore.daemonClient.loadDeviceSettings(device.uid)
            const deviceSettings = new DaemonDeviceSettings()
            deviceSettingsDTO.settings.forEach((setting: DeviceSettingReadDTO) =>
                deviceSettings.settings.set(setting.channel_name, setting),
            )
            allDaemonDeviceSettings.value.set(device.uid, deviceSettings)
        }
    }

    async function loadCCAllDeviceSettings(): Promise<void> {
        // The daemon returns all devices, both enabled and disabled.
        for (const deviceSetting of (await deviceStore.daemonClient.loadCCAllDeviceSettings())
            .devices) {
            ccDeviceSettings.value.set(deviceSetting.uid, deviceSetting)
            if (deviceSetting.disable) {
                ccBlacklistedDevices.value.set(deviceSetting.uid, deviceSetting)
            }
        }
    }

    /**
     * Loads all the Functions from the daemon. The default Function must be included.
     * These should be loaded before Profiles, as Profiles reference associated Functions.
     */
    async function loadFunctions(): Promise<void> {
        const functionsDTO = await deviceStore.daemonClient.loadFunctions()
        if (functionsDTO.functions.find((fun: Function) => fun.uid === '0') == null) {
            throw new Error(
                'Default Function not present in daemon Response. We should not continue.',
            )
        }
        functions.value.length = 0
        functions.value = functionsDTO.functions
    }

    /**
     * Saves the Functions order ONLY to the daemon.
     */
    async function saveFunctionsOrder(): Promise<void> {
        console.debug('Saving Functions Order')
        const functionsDTO = new FunctionsDTO()
        functionsDTO.functions = functions.value
        await deviceStore.daemonClient.saveFunctionsOrder(functionsDTO)
    }

    async function saveFunction(functionUID: UID): Promise<boolean> {
        console.debug('Saving Function')
        const fun_to_save = functions.value.find((fun) => fun.uid === functionUID)
        if (fun_to_save == null) {
            console.error('Function to save not found: ' + functionUID)
            return false
        }
        return await deviceStore.daemonClient.saveFunction(fun_to_save)
    }

    async function updateFunction(functionUID: UID): Promise<boolean> {
        console.debug('Updating Function')
        const fun_to_update = functions.value.find((fun) => fun.uid === functionUID)
        if (fun_to_update == null) {
            console.error('Function to update not found: ' + functionUID)
            return false
        }
        return await deviceStore.daemonClient.updateFunction(fun_to_update)
    }

    async function deleteFunction(functionUID: UID): Promise<void> {
        console.debug('Deleting Function')
        await deviceStore.daemonClient.deleteFunction(functionUID)
        await loadProfiles() // need to reload any changes to profiles from the Function removal
    }

    /**
     * Loads all the Profiles from the daemon. The default Profile must be included.
     */
    async function loadProfiles(): Promise<void> {
        const profilesDTO = await deviceStore.daemonClient.loadProfiles()
        if (profilesDTO.profiles.find((profile: Profile) => profile.uid === '0') == null) {
            throw new Error(
                'Unmanaged profile (UID 0) not present in daemon Response. We should not continue.',
            )
        }
        profiles.value.length = 0
        profiles.value = profilesDTO.profiles
    }

    /**
     * Saves the Profiles Order ONLY to the daemon.
     */
    async function saveProfilesOrder(): Promise<void> {
        console.debug('Saving Profiles Order')
        const profilesDTO = new ProfilesDTO()
        profilesDTO.profiles = profiles.value
        await deviceStore.daemonClient.saveProfilesOrder(profilesDTO)
    }

    async function saveProfile(profileUID: UID): Promise<boolean> {
        console.debug('Saving Profile')
        const profile_to_save = profiles.value.find((profile) => profile.uid === profileUID)
        if (profile_to_save == null) {
            console.error('Profile to save not found: ' + profileUID)
            return false
        }
        return await deviceStore.daemonClient.saveProfile(profile_to_save)
    }

    async function updateProfile(profileUID: UID): Promise<boolean> {
        console.debug('Updating Profile')
        const profile_to_update = profiles.value.find((profile) => profile.uid === profileUID)
        if (profile_to_update == null) {
            console.error('Profile to update not found: ' + profileUID)
            return false
        }
        return await deviceStore.daemonClient.updateProfile(profile_to_update)
    }

    async function deleteProfile(profileUID: UID): Promise<void> {
        console.debug('Deleting Profile')
        await deviceStore.daemonClient.deleteProfile(profileUID)
        await loadProfiles() // reload any changes to MixProfiles where this Profile was a member
        await loadModes() // reload Modes in case they contained this deleted Profile and changed
        await loadDaemonDeviceSettings()
    }

    async function loadModes(): Promise<void> {
        console.debug('Loading Modes')
        const modesDTO = await deviceStore.daemonClient.getModes()
        modes.value.length = 0
        modes.value = modesDTO.modes
        await syncSysTrayModes()
    }

    async function loadPowerProfiles(): Promise<void> {
        console.debug('Loading Power Profiles')
        const state = await deviceStore.daemonClient.getPowerProfiles()
        powerProfilesAvailable.value = state.available
        powerProfileActive.value = state.active ?? undefined
        powerProfileModes.value = state.modes
    }

    /**
     * Persists the power profile to Mode mapping. Profiles mapped to nothing are dropped, so
     * clearing a row removes it rather than storing an empty UID the daemon would reject.
     */
    async function savePowerProfileModes(modeMapping: Record<string, UID>): Promise<boolean> {
        console.debug('Saving Power Profile Modes')
        const mapped: Record<string, UID> = {}
        for (const [profile, modeUID] of Object.entries(modeMapping)) {
            if (modeUID) mapped[profile] = modeUID
        }
        const saved = await deviceStore.daemonClient.savePowerProfileModes(
            new PowerProfileModesDTO(mapped),
        )
        if (saved) powerProfileModes.value = mapped
        return saved
    }

    /** Tracks the active profile from the SSE `system` event so the page stays live. */
    function applySystemEvent(event: SystemEventDTO): void {
        if (event.kind !== 'power_profile') return
        powerProfileActive.value = event.value
    }

    async function saveModeOrder(): Promise<void> {
        console.debug('Saving Mode Order')
        const modeOrderDTO = new ModeOrderDTO()
        modeOrderDTO.mode_uids = modes.value.map((mode) => mode.uid)
        await deviceStore.daemonClient.saveModesOrder(modeOrderDTO)
        await syncSysTrayModes()
    }

    async function createMode(name: string): Promise<UID | undefined> {
        console.debug('Creating Mode')
        const createModeDTO = new CreateModeDTO(name)
        const response = await deviceStore.daemonClient.createMode(createModeDTO)
        if (response instanceof Mode) {
            const modeUID = response.uid
            modes.value.push(response)
            await syncSysTrayModes()
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.modeCreated'),
                life: 3000,
            })
            return modeUID
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return undefined
        }
    }

    async function duplicateMode(modeUID: UID): Promise<Mode | undefined> {
        console.debug('Duplicating Mode')
        const response = await deviceStore.daemonClient.duplicateMode(modeUID)
        if (response instanceof Mode) {
            modes.value.push(response)
            await syncSysTrayModes()
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.modeDuplicated'),
                life: 3000,
            })
            return response
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return undefined
        }
    }

    async function updateModeName(modeUID: UID, newName: string): Promise<boolean> {
        console.debug('Updating Mode')
        const updateModeDTO = new UpdateModeDTO(modeUID, newName)
        const response = await deviceStore.daemonClient.updateMode(updateModeDTO)
        if (response instanceof ErrorResponse) {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return false
        } else {
            const mode = modes.value.find((mode) => mode.uid === modeUID)
            if (mode != null) {
                mode.name = newName
            }
            await syncSysTrayModes()
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.modeNameUpdated'),
                life: 3000,
            })
            return true
        }
    }

    async function updateModeSettings(modeUID: UID): Promise<boolean> {
        console.debug('Updating Mode Settings')
        const response = await deviceStore.daemonClient.updateModeSettings(modeUID)
        if (response instanceof Mode) {
            const mode = modes.value.find((mode) => mode.uid === modeUID)
            if (mode != null) {
                mode.device_settings = response.device_settings
            }
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.modeUpdated'),
                life: 3000,
            })
            return true
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return false
        }
    }

    async function deleteMode(modeUID: UID): Promise<void> {
        console.debug('Deleting Mode')
        const response = await deviceStore.daemonClient.deleteMode(modeUID)
        if (response instanceof ErrorResponse) {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
        } else {
            const index = modes.value.findIndex((mode) => mode.uid === modeUID)
            if (index > -1) {
                modes.value.splice(index, 1)
            }
            await syncSysTrayModes()
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.modeDeleted'),
                life: 3000,
            })
        }
    }

    async function getActiveModes(): Promise<void> {
        console.debug('Getting Active Modes')
        const activeModes = await deviceStore.daemonClient.getActiveModeUIDs()
        modeActiveCurrent.value = activeModes.current_mode_uid
        modeActivePrevious.value = activeModes.previous_mode_uid
        emitter.emit('active-modes-change-menu')
    }

    async function activateMode(modeUID: UID): Promise<boolean> {
        console.debug('Activating Mode')
        const response = await deviceStore.daemonClient.activateMode(modeUID)
        if (response instanceof ErrorResponse) {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return false
        } else {
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.modeActivated'),
                life: 3000,
            })
            return true
        }
    }

    async function syncSysTrayModes(): Promise<void> {
        // This is used to refresh the Modes system tray menu contents: (from CRUD operations)
        if (deviceStore.isQtApp()) {
            const sysTrayModes = modes.value.map((mode) => {
                return { uid: mode.uid, name: mode.name }
            })
            // @ts-ignore
            const ipc = window.ipc
            await ipc.setModes(JSON.stringify({ modes: sysTrayModes }))
        }
    }

    async function getCustomSensors(): Promise<Array<CustomSensor>> {
        return await deviceStore.daemonClient.getCustomSensors()
    }

    /**
     * The function `getCustomSensor` retrieves a custom sensor object from the device store using a
     * custom sensor ID, and displays an error toast if the response is an `ErrorResponse`.
     * @param {string} customSensorID - The customSensorID parameter is a string that represents the
     * ID of a custom sensor.
     * @returns a Promise that resolves to either a CustomSensor object or undefined if there
     * was an error.
     */
    async function getCustomSensor(customSensorID: string): Promise<CustomSensor | undefined> {
        const response = await deviceStore.daemonClient.getCustomSensor(customSensorID)
        if (response instanceof CustomSensor) {
            return response
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
        }
    }

    /**
     * The function saves a custom sensor by calling a method from the deviceStore daemon client.
     * @param {CustomSensor} newCustomSensor - The parameter `newCustomSensor` is of type
     * `CustomSensor`.
     * @returns a Promise<boolean>.
     */
    async function saveCustomSensor(newCustomSensor: CustomSensor): Promise<boolean> {
        console.debug('Saving Custom Sensor')
        const response = await deviceStore.daemonClient.saveCustomSensor(newCustomSensor)
        if (response == null) {
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.customSensorSaved'),
                life: 3000,
            })
            return true
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return false
        }
    }

    /**
     * The function `updateCustomSensor` updates a custom sensor and returns a boolean indicating if
     * the update was successful.
     * @param {CustomSensor} customSensor - The customSensor parameter is an object that represents a
     * custom sensor.
     * @returns a Promise<boolean>.
     */
    async function updateCustomSensor(customSensor: CustomSensor): Promise<boolean> {
        console.debug('Updating Custom Sensor')
        const response = await deviceStore.daemonClient.updateCustomSensor(customSensor)
        if (response == null) {
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.customSensorUpdated'),
                life: 3000,
            })
            return true
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return false
        }
    }

    /**
     * The function `deleteCustomSensor` is an asynchronous function that deletes a custom sensor
     * and refreshed the UI if successful.
     * @param {UID} deviceUID - The deviceUID parameter is the unique identifier of the custom
     * sensor's device. Used to remove any associated user UI settings as well.
     * @param {string} customSensorID - The `customSensorID` parameter is a string that represents
     * the unique identifier of the custom sensor that you want to delete.
     */
    async function deleteCustomSensor(deviceUID: UID, customSensorID: string): Promise<void> {
        console.debug('Deleting Custom Sensor')
        const response = await deviceStore.daemonClient.deleteCustomSensor(customSensorID)
        if (response == null) {
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.customSensorDeleted'),
                life: 3000,
            })
            allUIDeviceSettings.value
                .get(deviceUID)!
                .sensorsAndChannels.get(customSensorID)!.userName = undefined
            allUIDeviceSettings.value
                .get(deviceUID)!
                .sensorsAndChannels.get(customSensorID)!.userColor = undefined
            await deviceStore.waitAndReload()
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
        }
    }

    async function loadAlertsAndLogs(): Promise<void> {
        console.debug('Loading Alerts')
        const alertsDTO = await deviceStore.daemonClient.loadAlertsAndLogs()
        alertsActive.value.length = 0
        alertsError.value.length = 0
        for (const alert of alertsDTO.alerts) {
            if (alert.state === AlertState.Active) alertsActive.value.push(alert.uid)
            else if (alert.state === AlertState.Error) alertsError.value.push(alert.uid)
        }
        alerts.value.length = 0
        alerts.value = alertsDTO.alerts
        alertLogs.value.length = 0
        alertLogs.value = alertsDTO.logs
    }

    async function loadDeviceHealth(): Promise<void> {
        console.debug('Loading Device Health')
        applyDeviceHealthSnapshot(await deviceStore.daemonClient.loadDeviceHealth())
    }

    function applyDeviceHealthSnapshot(health: DeviceHealthDTO): void {
        healthFailsafe.value = health.failsafe
        healthUnreachable.value = health.unreachable
        healthMissing.value = health.missing
        healthStaleSource.value = health.stale_source
        healthChannelCapabilities.value = health.channel_capabilities
        healthFirmwareOverrides.value = health.firmware_overrides
        healthSystemFindings.value = health.system_findings
    }

    function applyFailsafeDelta(delta: FailsafeDelta): void {
        const index = healthFailsafe.value.findIndex(
            (ref) => failsafeKey(ref) === failsafeKey(delta),
        )
        if (delta.state === HealthState.Detected && index === -1) {
            healthFailsafe.value.push(delta)
        } else if (delta.state === HealthState.Resolved && index > -1) {
            healthFailsafe.value.splice(index, 1)
        }
    }

    function applyUnreachableDelta(delta: UnreachableDelta): void {
        const index = healthUnreachable.value.findIndex(
            (ref) => ref.device_uid === delta.device_uid,
        )
        if (delta.state === HealthState.Detected && index === -1) {
            healthUnreachable.value.push(delta)
        } else if (delta.state === HealthState.Resolved && index > -1) {
            healthUnreachable.value.splice(index, 1)
        }
    }

    function applyMissingDelta(delta: SourceDelta): void {
        const index = healthMissing.value.findIndex((ref) => sourceKey(ref) === sourceKey(delta))
        if (delta.state === HealthState.Detected && index === -1) {
            healthMissing.value.push(delta)
        } else if (delta.state === HealthState.Resolved && index > -1) {
            healthMissing.value.splice(index, 1)
        }
    }

    function applyStaleSourceDelta(delta: SourceDelta): void {
        const index = healthStaleSource.value.findIndex(
            (ref) => sourceKey(ref) === sourceKey(delta),
        )
        if (delta.state === HealthState.Detected && index === -1) {
            healthStaleSource.value.push(delta)
        } else if (delta.state === HealthState.Resolved && index > -1) {
            healthStaleSource.value.splice(index, 1)
        }
    }

    async function createAlert(alert: Alert): Promise<boolean> {
        console.debug('Creating Alert')
        const response = await deviceStore.daemonClient.createAlert(alert)
        if (response == null) {
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.alertSaved'),
                life: 3000,
            })
            return true
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return false
        }
    }

    async function updateAlert(alertUID: UID): Promise<boolean> {
        console.debug('Updating Alert')
        const alert_to_update = alerts.value.find((alert) => alert.uid === alertUID)
        if (alert_to_update == null) {
            console.error('Alert to update not found: ' + alertUID)
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: t('common.toast.alertNotFound'),
                life: 4000,
            })
            return false
        }
        const response = await deviceStore.daemonClient.updateAlert(alert_to_update)
        if (response == null) {
            // The daemon resets a disabled alert to Inactive but does so silently (no
            // SSE event), so mirror that locally to keep the active set, top-bar badge,
            // and panel counter consistent. Re-enabling is announced via SSE.
            if (!alert_to_update.enabled) {
                alert_to_update.state = AlertState.Inactive
                clearAlertAttention(alertUID)
            }
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.alertUpdated'),
                life: 3000,
            })
            return true
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return false
        }
    }

    // Alert quiet controls. These own the silence/enable contract (timestamp
    // math, unsilence = undefined) and roll the optimistic mutation back if
    // the daemon rejects the update.
    async function silenceAlert(alertUID: UID, minutes: number): Promise<boolean> {
        const alert = alerts.value.find((entry) => entry.uid === alertUID)
        if (alert == null) return false
        const previous = alert.silenced_until
        alert.silenced_until = new Date(Date.now() + minutes * 60_000).toISOString()
        const successful = await updateAlert(alertUID)
        if (!successful) alert.silenced_until = previous
        return successful
    }

    async function unsilenceAlert(alertUID: UID): Promise<boolean> {
        const alert = alerts.value.find((entry) => entry.uid === alertUID)
        if (alert == null) return false
        const previous = alert.silenced_until
        alert.silenced_until = undefined
        const successful = await updateAlert(alertUID)
        if (!successful) alert.silenced_until = previous
        return successful
    }

    async function setAlertEnabled(alertUID: UID, enabled: boolean): Promise<boolean> {
        const alert = alerts.value.find((entry) => entry.uid === alertUID)
        if (alert == null) return false
        const previous = alert.enabled
        alert.enabled = enabled
        const successful = await updateAlert(alertUID)
        if (!successful) alert.enabled = previous
        return successful
    }

    async function deleteAlert(alertUID: UID): Promise<boolean> {
        console.debug('Deleting Alert')
        const response = await deviceStore.daemonClient.deleteAlert(alertUID)
        if (response == null) {
            const index = alerts.value.findIndex((alert) => alert.uid === alertUID)
            if (index > -1) {
                alerts.value.splice(index, 1)
            }
            clearAlertAttention(alertUID)
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.alertDeleted'),
                life: 3000,
            })
            return true
        } else {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
            return false
        }
    }

    /**
     * This needs to be called after everything is initialized and setup, then we can sync all UI settings automatically.
     */
    async function startWatchingToSaveChanges() {
        watch(
            [
                allUIDeviceSettings.value,
                dashboards,
                homeDashboard,
                chartLineScale,
                startInSystemTray,
                closeToSystemTray,
                desktopStartupDelay,
                themeMode,
                uiScale,
                time24,
                menuOrder,
                libraryFolderNames,
                expandedMenuIds,
                pinnedIds,
                collapsedMainMenu,
                hideMenuCollapseIcon,
                mainMenuWidthRem,
                frequencyPrecision,
                customTheme,
                entityColors.value,
                eyeCandy,
                pointsOverlayTablePositions,
                interfaceFont,
                language,
                onboardingSeenVersion,
                cpuStressBackend,
                gpuStressBackend,
                ramStressBackend,
                driveStressBackend,
                startupPage,
                tags.value,
            ],
            _.debounce(
                // we debounce to not continuously save changes
                async () => {
                    console.debug('Saving UI Settings')
                    const uiSettings = new UISettingsDTO()
                    for (const [uid, deviceSettings] of allUIDeviceSettings.value) {
                        uiSettings.devices?.push(toRaw(uid))
                        const deviceSettingsDto = new DeviceUISettingsDTO()
                        deviceSettingsDto.userName = deviceSettings.userName
                        deviceSettingsDto.userColor = deviceSettings.userColor
                        deviceSettings.sensorsAndChannels.forEach(
                            (sensorAndChannelSettings, name) => {
                                deviceSettingsDto.names.push(name)
                                deviceSettingsDto.sensorAndChannelSettings.push(
                                    sensorAndChannelSettings,
                                )
                            },
                        )
                        uiSettings.deviceSettings?.push(deviceSettingsDto)
                    }
                    uiSettings.dashboards = dashboards
                    uiSettings.homeDashboard = homeDashboard.value
                    uiSettings.chartLineScale = chartLineScale.value
                    if (deviceStore.isQtApp()) {
                        try {
                            // @ts-ignore
                            const ipc = window.ipc
                            await ipc.setStartInTray(startInSystemTray.value)
                            await ipc.setStartupDelay(desktopStartupDelay.value)
                            await ipc.setCloseToTray(closeToSystemTray.value)
                            await ipc.setZoomFactor(uiScale.value / 100)
                        } catch (e) {
                            console.error('Failed to set Desktop settings: ', e)
                        }
                    }
                    uiSettings.themeMode = themeMode.value
                    uiSettings.time24 = time24.value
                    uiSettings.menuOrder = menuOrder.value
                    uiSettings.libraryFolderNames = libraryFolderNames.value
                    uiSettings.expandedMenuIds = expandedMenuIds.value
                    uiSettings.pinnedIds = pinnedIds.value
                    uiSettings.collapsedMainMenu = collapsedMainMenu.value
                    uiSettings.hideMenuCollapseIcon = hideMenuCollapseIcon.value
                    uiSettings.mainMenuWidthRem = mainMenuWidthRem.value
                    uiSettings.frequencyPrecision = frequencyPrecision.value
                    for (const key of THEME_TOKEN_KEYS) {
                        uiSettings.customTheme[key] = customTheme[key]
                    }
                    uiSettings.entityColors = entityColors.value
                    uiSettings.eyeCandy = eyeCandy.value
                    uiSettings.pointsOverlayTablePositions = pointsOverlayTablePositions.value
                    uiSettings.interfaceFont = interfaceFont.value
                    uiSettings.language = language.value
                    uiSettings.showOnboarding = onboardingSeenVersion.value
                    uiSettings.cpuStressBackend = cpuStressBackend.value
                    uiSettings.gpuStressBackend = gpuStressBackend.value
                    uiSettings.ramStressBackend = ramStressBackend.value
                    uiSettings.driveStressBackend = driveStressBackend.value
                    uiSettings.startupPage = startupPage.value
                    tags.value.forEach((tagSettings, name) => {
                        uiSettings.tagNames.push(name)
                        uiSettings.tagColors.push(tagSettings.color)
                    })
                    await deviceStore.daemonClient.saveUISettings(uiSettings)
                },
                750,
            ),
        )

        watch(ccSettings.value, async () => {
            console.debug('Saving CC Settings')
            await deviceStore.daemonClient.saveCCSettings(ccSettings.value)
        })
    }

    // Deliberately after the save watcher starts: a value adopted from an older
    // version's localStorage is a change the daemon has never seen, and it has
    // to be written there or the next lost browser store forgets it again. The
    // language on screen is already right, having come from the same cache at
    // module load, so this only flips it when the daemon disagrees.
    function adoptLanguageSetting(): void {
        language.value = languageSetting(persistedLanguage)
        applyLanguage()
    }

    // The daemon holds the setting; localStorage mirrors it so the next start
    // paints in the right language before the settings request returns, and
    // <html lang> follows so the browser hyphenates and speaks the right one.
    function applyLanguage(): void {
        const resolved = resolveLanguage(language.value)
        i18n.global.locale.value = resolved
        document.documentElement.setAttribute('lang', resolved)
        cacheLanguage(language.value)
    }

    // Both font roles live in CSS variables, so the setting is one class.
    function applyInterfaceFont(): void {
        document.documentElement.classList.toggle(
            'system-fonts',
            interfaceFont.value === InterfaceFont.SYSTEM,
        )
    }

    function applyThemeMode(): void {
        document.documentElement.classList.remove('high-contrast-dark')
        document.documentElement.classList.remove('high-contrast-light')
        document.documentElement.classList.remove('light-theme')
        document.documentElement.classList.remove('dark-theme')
        document.documentElement.classList.remove('custom-theme')
        document.documentElement.classList.remove('installed-theme')

        // Clear the variables the custom and installed themes set, so the next
        // theme's compiled values are not shadowed by the previous one.
        for (const cssVar of THEME_CSS_VAR_NAMES) {
            document.documentElement.style.removeProperty(cssVar)
        }

        // Installed themes carry their whole palette, so they take no compiled
        // theme class: the neutral one leaves the base palette to supply the
        // global hues and the variables below win over it.
        const theme = installedTheme(themeMode.value)
        if (theme != null) {
            document.documentElement.classList.add('installed-theme')
            for (const [cssVar, value] of themeCssVars(theme)) {
                document.documentElement.style.setProperty(cssVar, value)
            }
            return
        }

        if (themeMode.value === ThemeMode.SYSTEM) {
            const palette = systemPalette.value
            // A desktop that publishes its whole palette gets applied exactly like
            // an installed theme: same variables, same neutral class.
            if (palette?.tokens != null) {
                document.documentElement.classList.add('installed-theme')
                const asTheme = {
                    id: SYSTEM_THEME_ID,
                    name: 'System',
                    variant: palette.variant ?? 'dark',
                    tokens: palette.tokens,
                } as const
                for (const [cssVar, value] of themeCssVars(asTheme)) {
                    document.documentElement.style.setProperty(cssVar, value)
                }
                return
            }
            // The desktop's own answer beats the media query: Chromium's light/dark
            // detection is unreliable under GNOME, where it reports light on a dark
            // session. The query is still the fallback for browsers and for a
            // desktop that states no preference.
            const prefersDark =
                palette?.variant != null
                    ? palette.variant === 'dark'
                    : !window.matchMedia('(prefers-color-scheme: light)').matches
            if (window.matchMedia('(prefers-contrast: more)').matches) {
                document.documentElement.classList.add(
                    prefersDark ? 'high-contrast-dark' : 'high-contrast-light',
                )
            } else {
                document.documentElement.classList.add(prefersDark ? 'dark-theme' : 'light-theme')
            }
            // Only an accent is available here, so it is layered over the compiled
            // theme. Both ends of the gradient take it: the desktop has no second
            // brand color to give. The accent foreground recomputes on its own, off
            // the style change this makes (see ThemeColorsStore).
            if (palette?.accent != null) {
                const accent = hexToTriplet(palette.accent)
                document.documentElement.style.setProperty(THEME_TOKEN_VARS.accent, accent)
                document.documentElement.style.setProperty(
                    THEME_TOKEN_VARS.accentGradientTo,
                    accent,
                )
            }
        } else if (themeMode.value === ThemeMode.HIGH_CONTRAST_DARK) {
            document.documentElement.classList.add('high-contrast-dark')
        } else if (themeMode.value === ThemeMode.HIGH_CONTRAST_LIGHT) {
            document.documentElement.classList.add('high-contrast-light')
        } else if (themeMode.value === ThemeMode.LIGHT) {
            document.documentElement.classList.add('light-theme')
        } else if (themeMode.value === ThemeMode.CUSTOM) {
            document.documentElement.classList.add('custom-theme')
            for (const key of THEME_TOKEN_KEYS) {
                document.documentElement.style.setProperty(THEME_TOKEN_VARS[key], customTheme[key])
            }
            // The variant is not declared for a custom theme, so read it off the
            // chosen background: a light one has to darken on hover, not wash out.
            document.documentElement.style.setProperty(
                '--colors-surface-hover',
                surfaceTintFor(customTheme.bgOne),
            )
        } else {
            document.documentElement.classList.add('dark-theme')
        }
    }

    async function handleSaveDeviceSettingResponse(
        deviceUID: UID,
        successful: boolean,
        errorMsg: string | undefined = undefined,
    ): Promise<void> {
        if (successful) {
            await loadDaemonDeviceSettings(deviceUID)
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.settingsUpdated'),
                life: 3000,
            })
        } else {
            const message = errorMsg != null ? errorMsg : t('common.toast.settingsError')
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: message,
                life: 4000,
            })
        }
        console.debug('Daemon Settings Saved')
    }

    async function saveDaemonDeviceSettingManual(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteManualDTO,
    ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingManual(
            deviceUID,
            channelName,
            setting,
        )
        await handleSaveDeviceSettingResponse(deviceUID, successful)
    }

    async function saveDaemonDeviceSettingProfile(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteProfileDTO,
    ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingProfile(
            deviceUID,
            channelName,
            setting,
        )
        await handleSaveDeviceSettingResponse(deviceUID, successful)
    }

    async function saveDaemonDeviceSettingLcd(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteLcdDTO,
    ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingLcd(
            deviceUID,
            channelName,
            setting,
        )
        await handleSaveDeviceSettingResponse(deviceUID, successful)
    }

    async function saveDaemonDeviceSettingLcdImages(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteLcdDTO,
        files: Array<File>,
    ): Promise<void> {
        const response = await deviceStore.daemonClient.saveDeviceSettingLcdImages(
            deviceUID,
            channelName,
            setting,
            files,
        )
        const successful = response === undefined
        await handleSaveDeviceSettingResponse(deviceUID, successful, response?.error)
    }

    async function saveDaemonDeviceSettingLighting(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteLightingDTO,
    ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingLighting(
            deviceUID,
            channelName,
            setting,
        )
        await handleSaveDeviceSettingResponse(deviceUID, successful)
    }

    async function saveDaemonDeviceSettingPWM(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWritePWMModeDTO,
    ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingPWM(
            deviceUID,
            channelName,
            setting,
        )
        await handleSaveDeviceSettingResponse(deviceUID, successful)
    }

    async function saveDaemonDeviceSettingReset(
        deviceUID: UID,
        channelName: string,
    ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingReset(
            deviceUID,
            channelName,
        )
        await handleSaveDeviceSettingResponse(deviceUID, successful)
    }

    async function applyThinkPadFanControl(enable: boolean): Promise<void> {
        const response: undefined | ErrorResponse =
            await deviceStore.daemonClient.thinkPadFanControl(enable)
        if (response instanceof ErrorResponse) {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: response.error,
                life: 4000,
            })
        } else {
            // the daemon keeps its own device info in sync, this reflects it
            // in the UI without waiting for a device reload
            thinkPadFanControlEnabled.value = enable
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.thinkPadFanControlApplied'),
                life: 3000,
            })
        }
    }

    function createTag(name: string, color: Color): void {
        if (!tags.value.has(name)) {
            tags.value.set(name, new TagSettings(name, color))
        }
    }

    function deleteTag(name: string): void {
        tags.value.delete(name)
        // remove from all channels
        allUIDeviceSettings.value.forEach((deviceSettings) => {
            deviceSettings.sensorsAndChannels.forEach((channelSettings) => {
                const idx = channelSettings.tags.indexOf(name)
                if (idx > -1) {
                    channelSettings.tags.splice(idx, 1)
                }
            })
        })
    }

    function reorderTags(orderedNames: Array<string>): void {
        const entries: Array<[string, TagSettings]> = []
        for (const name of orderedNames) {
            const tag = tags.value.get(name)
            if (tag != null) entries.push([name, tag])
        }
        tags.value.clear()
        for (const [name, tag] of entries) {
            tags.value.set(name, tag)
        }
    }

    function updateTagColor(name: string, color: Color): void {
        if (!tags.value.has(name)) return
        tags.value.set(name, new TagSettings(name, color))
    }

    function renameTag(oldName: string, newName: string): void {
        if (!tags.value.has(oldName) || tags.value.has(newName)) return
        const tagSettings = tags.value.get(oldName)!
        tagSettings.name = newName
        // Rebuild the Map to preserve insertion order
        const entries = Array.from(tags.value.entries())
        tags.value.clear()
        for (const [name, settings] of entries) {
            if (name === oldName) {
                tags.value.set(newName, tagSettings)
            } else {
                tags.value.set(name, settings)
            }
        }
        allUIDeviceSettings.value.forEach((deviceSettings) => {
            deviceSettings.sensorsAndChannels.forEach((channelSettings) => {
                const idx = channelSettings.tags.indexOf(oldName)
                if (idx > -1) {
                    channelSettings.tags[idx] = newName
                }
            })
        })
    }

    function assignTagToChannel(deviceUID: UID, channelName: string, tagName: string): void {
        const channelSettings = allUIDeviceSettings.value
            .get(deviceUID)
            ?.sensorsAndChannels.get(channelName)
        if (channelSettings == null) return
        if (!channelSettings.tags.includes(tagName)) {
            channelSettings.tags.push(tagName)
        }
    }

    function removeTagFromChannel(deviceUID: UID, channelName: string, tagName: string): void {
        const channelSettings = allUIDeviceSettings.value
            .get(deviceUID)
            ?.sensorsAndChannels.get(channelName)
        if (channelSettings == null) return
        const idx = channelSettings.tags.indexOf(tagName)
        if (idx > -1) {
            channelSettings.tags.splice(idx, 1)
        }
    }

    function getChannelTags(deviceUID: UID, channelName: string): Array<string> {
        const channelTags =
            allUIDeviceSettings.value.get(deviceUID)?.sensorsAndChannels.get(channelName)?.tags ??
            []
        if (channelTags.length <= 1) return channelTags
        const tagOrder = Array.from(tags.value.keys())
        return [...channelTags].sort((a, b) => tagOrder.indexOf(a) - tagOrder.indexOf(b))
    }

    function getTagChannels(tagName: string): Array<{ deviceUID: string; channelName: string }> {
        const result: Array<{ deviceUID: string; channelName: string }> = []
        allUIDeviceSettings.value.forEach((deviceSettings, deviceUID) => {
            deviceSettings.sensorsAndChannels.forEach((channelSettings, channelName) => {
                if (channelSettings.tags.includes(tagName)) {
                    result.push({ deviceUID, channelName })
                }
            })
        })
        return result
    }

    console.debug(`Settings Store created`)
    return {
        initializeSettings,
        loadDaemonDeviceSettings,
        nameOverrides,
        saveDeviceName,
        saveChannelName,
        defaultDeviceName,
        defaultChannelLabel,
        predefinedColorOptions,
        profiles,
        functions,
        modes,
        modeActiveCurrent,
        powerProfilesAvailable,
        powerProfileActive,
        powerProfileModes,
        loadPowerProfiles,
        savePowerProfileModes,
        applySystemEvent,
        modeActivePrevious,
        modeInEdit,
        allUIDeviceSettings,
        dashboards,
        homeDashboard,
        chartLineScale,
        startInSystemTray,
        closeToSystemTray,
        desktopStartupDelay,
        themeMode,
        systemPalette,
        uiScale,
        time24,
        menuOrder,
        libraryFolderNames,
        expandedMenuIds,
        pinnedIds,
        collapsedMainMenu,
        hideMenuCollapseIcon,
        mainMenuWidthRem,
        frequencyPrecision,
        customTheme,
        entityColors,
        eyeCandy,
        pointsTablePosition,
        setPointsTablePosition,
        interfaceFont,
        language,
        showOnboarding,
        completeOnboarding,
        cpuStressBackend,
        gpuStressBackend,
        ramStressBackend,
        driveStressBackend,
        startupPage,
        allDaemonDeviceSettings,
        ccSettings,
        ccDeviceSettings,
        ccBlacklistedDevices,
        thinkPadFanControlEnabled,
        applyThinkPadFanControl,
        saveDaemonDeviceSettingManual,
        saveDaemonDeviceSettingProfile,
        saveDaemonDeviceSettingLcd,
        saveDaemonDeviceSettingLcdImages,
        saveDaemonDeviceSettingLighting,
        saveDaemonDeviceSettingPWM,
        saveDaemonDeviceSettingReset,
        saveFunctionsOrder,
        saveFunction,
        updateFunction,
        deleteFunction,
        saveProfilesOrder,
        saveProfile,
        updateProfile,
        deleteProfile,
        saveModeOrder,
        createMode,
        duplicateMode,
        updateModeName,
        updateModeSettings,
        deleteMode,
        getActiveModes,
        activateMode,
        getCustomSensors,
        getCustomSensor,
        saveCustomSensor,
        updateCustomSensor,
        deleteCustomSensor,
        alerts,
        alertLogs,
        alertsActive,
        alertsError,
        alertsNeedingAttention,
        clearAlertAttention,
        anyActiveUnsilencedAlert,
        pushTrayAlertState,
        pushTrayPinnedSensors,
        loadAlertsAndLogs,
        createAlert,
        updateAlert,
        silenceAlert,
        unsilenceAlert,
        setAlertEnabled,
        deleteAlert,
        healthFailsafe,
        healthUnreachable,
        healthChannelCapabilities,
        healthFirmwareOverrides,
        healthSystemFindings,
        channelVerdict,
        healthMissing,
        healthStaleSource,
        loadDeviceHealth,
        applyDeviceHealthSnapshot,
        applyFailsafeDelta,
        applyUnreachableDelta,
        applyMissingDelta,
        applyStaleSourceDelta,
        applyThemeMode,
        applyInterfaceFont,
        applyLanguage,
        tags,
        createTag,
        deleteTag,
        reorderTags,
        updateTagColor,
        renameTag,
        assignTagToChannel,
        removeTagFromChannel,
        getChannelTags,
        getTagChannels,
    }
})
