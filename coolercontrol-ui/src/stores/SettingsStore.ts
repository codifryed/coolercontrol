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

import { defineStore } from 'pinia'
import { Function, FunctionsDTO, Profile, ProfilesDTO } from '@/models/Profile'
import type { Ref } from 'vue'
import { reactive, inject, ref, toRaw, watch } from 'vue'
import {
    type AllDeviceSettings,
    CustomThemeSettings,
    defaultCustomTheme,
    DeviceUISettings,
    DeviceUISettingsDTO,
    SensorAndChannelSettings,
    ThemeMode,
    UISettingsDTO,
} from '@/models/UISettings'
import type { UID } from '@/models/Device'
import { Device } from '@/models/Device'
import setDefaultSensorAndChannelColors from '@/stores/DeviceColorCreator'
import { useDeviceStore } from '@/stores/DeviceStore'
import type { AllDaemonDeviceSettings } from '@/models/DaemonSettings'
import {
    DaemonDeviceSettings,
    DeviceSettingReadDTO,
    DeviceSettingWriteLcdDTO,
    DeviceSettingWriteLightingDTO,
    DeviceSettingWriteManualDTO,
    DeviceSettingWriteProfileDTO,
    DeviceSettingWritePWMModeDTO,
} from '@/models/DaemonSettings'
import { useToast } from 'primevue/usetoast'
import { CoolerControlDeviceSettingsDTO, CoolerControlSettingsDTO } from '@/models/CCSettings'
import { ErrorResponse } from '@/models/ErrorResponse'
import { CustomSensor } from '@/models/CustomSensor'
import { CreateModeDTO, Mode, ModeOrderDTO, UpdateModeDTO } from '@/models/Mode.ts'
import { Dashboard } from '@/models/Dashboard.ts'
import { Emitter, EventType } from 'mitt'
import _ from 'lodash'
import { Alert, AlertLog, AlertState } from '@/models/Alert.ts'
import { useI18n } from 'vue-i18n'

export const useSettingsStore = defineStore('settings', () => {
    const toast = useToast()
    const { t } = useI18n()

    const deviceStore = useDeviceStore() // using another store internally in this way seems ok, as long as we don't have a circular dependency
    const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
    const predefinedColorOptions: Ref<Array<string>> = ref([
        '#FFFFFF',
        '#000000',
        '#FF0000',
        '#FFFF00',
        '#00FF00',
        '#FF00FF',
        '#00FFFF',
        '#0000FF',
    ])

    const functions: Ref<Array<Function>> = ref([])

    const profiles: Ref<Array<Profile>> = ref([])

    const modes: Ref<Array<Mode>> = ref([])

    const modeActiveCurrent: Ref<UID | undefined> = ref()
    const modeActivePrevious: Ref<UID | undefined> = ref()

    const modeInEdit: Ref<UID | undefined> = ref()

    const alerts: Ref<Array<Alert>> = ref([])
    const alertLogs: Ref<Array<AlertLog>> = ref([])
    const alertsActive: Ref<Array<UID>> = ref([])

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

    const dashboards: Array<Dashboard> = reactive([...Dashboard.defaults()])
    const chartLineScale: Ref<number> = ref(1.5)
    const startInSystemTray: Ref<boolean> = ref(false)
    const closeToSystemTray: Ref<boolean> = ref(false)
    const desktopStartupDelay: Ref<number> = ref(0)
    const themeMode: Ref<ThemeMode> = ref(ThemeMode.SYSTEM)
    const uiScale: Ref<number> = ref(100)
    const time24: Ref<boolean> = ref(false)
    const collapsedMenuNodeIds: Ref<Array<string>> = ref([])
    const collapsedMainMenu: Ref<boolean> = ref(false)
    const hideMenuCollapseIcon: Ref<boolean> = ref(false)
    const menuEntitiesAtBottom: Ref<boolean> = ref(false)
    const mainMenuWidthRem: Ref<number> = ref(24)
    const frequencyPrecision: Ref<number> = ref(1)
    const customTheme: CustomThemeSettings = reactive({
        accent: defaultCustomTheme.accent,
        bgOne: defaultCustomTheme.bgOne,
        bgTwo: defaultCustomTheme.bgTwo,
        borderOne: defaultCustomTheme.borderOne,
        textColor: defaultCustomTheme.textColor,
        textColorSecondary: defaultCustomTheme.textColorSecondary,
    })
    const showOnboarding: Ref<boolean> = ref(true)

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
        chartLineScale.value = uiSettings.chartLineScale
        if (deviceStore.isQtApp()) {
            // @ts-ignore
            const ipc = window.ipc
            try {
                startInSystemTray.value = await ipc.getStartInTray()
                desktopStartupDelay.value = await ipc.getStartupDelay()
                closeToSystemTray.value = await ipc.getCloseToTray()
                uiScale.value = (await ipc.getZoomFactor()) * 100
            } catch (err: any) {
                console.error('Failed to get desktop setting: ', err)
            }
        }
        themeMode.value = uiSettings.themeMode
        applyThemeMode()
        time24.value = uiSettings.time24
        collapsedMenuNodeIds.value = uiSettings.collapsedMenuNodeIds
        collapsedMainMenu.value = uiSettings.collapsedMainMenu
        mainMenuWidthRem.value = uiSettings.mainMenuWidthRem
        hideMenuCollapseIcon.value = uiSettings.hideMenuCollapseIcon
        menuEntitiesAtBottom.value = uiSettings.menuEntitiesAtBottom
        frequencyPrecision.value = uiSettings.frequencyPrecision
        customTheme.accent = uiSettings.customTheme.accent
        customTheme.bgOne = uiSettings.customTheme.bgOne
        customTheme.bgTwo = uiSettings.customTheme.bgTwo
        customTheme.borderOne = uiSettings.customTheme.borderOne
        customTheme.textColor = uiSettings.customTheme.textColor
        customTheme.textColorSecondary = uiSettings.customTheme.textColorSecondary
        showOnboarding.value = uiSettings.showOnboarding
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
                deviceSettings.menuCollapsed = deviceSettingsDto.menuCollapsed
                deviceSettings.userName = deviceSettingsDto.userName
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
        setDisplayNames(allDevices, allUIDeviceSettings.value)
        await loadDaemonDeviceSettings()
        await loadCCAllDeviceSettings()

        await loadAlertsAndLogs()
        await loadFunctions()
        await loadProfiles()
        await loadModes()
        await getActiveModes()

        await startWatchingToSaveChanges()
    }

    async function loadCCSettings(): Promise<void> {
        ccSettings.value = await deviceStore.daemonClient.loadCCSettings()
    }

    function setDisplayNames(
        devices: Array<Device>,
        deviceSettings: Map<UID, DeviceUISettings>,
    ): void {
        for (const device of devices) {
            const settings = deviceSettings.get(device.uid)!
            // Default display name takes the model name if it's available, before the driver name (HWMon especially):
            settings.displayName =
                device.info?.model != null && device.info.model.length > 0
                    ? device.info.model
                    : device.nameShort
            if (device.status_history.length) {
                for (const channelStatus of device.status.channels) {
                    if (channelStatus.name.toLowerCase().includes('load')) {
                        settings.sensorsAndChannels.get(channelStatus.name)!.channelLabel =
                            channelStatus.name
                    }
                }
            }
            if (device.info != null) {
                for (const [channelName, channelInfo] of device.info.channels.entries()) {
                    if (channelInfo.speed_options != null) {
                        settings.sensorsAndChannels.get(channelName)!.channelLabel =
                            channelInfo.label != null
                                ? channelInfo.label
                                : deviceStore.toTitleCase(channelName)
                    } else if (channelInfo.lighting_modes.length > 0) {
                        settings.sensorsAndChannels.get(channelName)!.channelLabel =
                            deviceStore.toTitleCase(channelName)
                    } else if (channelInfo.lcd_modes.length > 0) {
                        settings.sensorsAndChannels.get(channelName)!.channelLabel =
                            channelName.toUpperCase()
                    } else {
                        // must be Frequency
                        settings.sensorsAndChannels.get(channelName)!.channelLabel =
                            channelInfo.label != null
                                ? channelInfo.label
                                : deviceStore.toTitleCase(channelName)
                    }
                }
                for (const [tempName, tempInfo] of device.info.temps.entries()) {
                    if (settings.sensorsAndChannels.get(tempName) != null) {
                        settings.sensorsAndChannels.get(tempName)!.channelLabel = tempInfo.label
                    }
                }
            }
        }
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
                'Default Profile not present in daemon Response. We should not continue.',
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
        alertsDTO.alerts
            .filter((alert) => alert.state === AlertState.Active)
            .forEach((alert) => {
                alertsActive.value.push(alert.uid)
            })
        alerts.value.length = 0
        alerts.value = alertsDTO.alerts
        alertLogs.value.length = 0
        alertLogs.value = alertsDTO.logs
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

    async function deleteAlert(alertUID: UID): Promise<boolean> {
        console.debug('Deleting Alert')
        const response = await deviceStore.daemonClient.deleteAlert(alertUID)
        if (response == null) {
            const index = alerts.value.findIndex((alert) => alert.uid === alertUID)
            if (index > -1) {
                alerts.value.splice(index, 1)
            }
            const activeIndex = alertsActive.value.findIndex((uid) => uid === alertUID)
            if (activeIndex > -1) {
                alertsActive.value.splice(activeIndex, 1)
            }
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
                chartLineScale,
                startInSystemTray,
                closeToSystemTray,
                desktopStartupDelay,
                themeMode,
                uiScale,
                time24,
                collapsedMenuNodeIds.value,
                collapsedMainMenu,
                hideMenuCollapseIcon,
                menuEntitiesAtBottom,
                mainMenuWidthRem,
                frequencyPrecision,
                customTheme,
                showOnboarding,
            ],
            _.debounce(
                // we debounce to not continuously save changes
                async () => {
                    console.debug('Saving UI Settings')
                    const uiSettings = new UISettingsDTO()
                    for (const [uid, deviceSettings] of allUIDeviceSettings.value) {
                        uiSettings.devices?.push(toRaw(uid))
                        const deviceSettingsDto = new DeviceUISettingsDTO()
                        deviceSettingsDto.menuCollapsed = deviceSettings.menuCollapsed
                        deviceSettingsDto.userName = deviceSettings.userName
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
                    uiSettings.collapsedMenuNodeIds = collapsedMenuNodeIds.value
                    uiSettings.collapsedMainMenu = collapsedMainMenu.value
                    uiSettings.hideMenuCollapseIcon = hideMenuCollapseIcon.value
                    uiSettings.menuEntitiesAtBottom = menuEntitiesAtBottom.value
                    uiSettings.mainMenuWidthRem = mainMenuWidthRem.value
                    uiSettings.frequencyPrecision = frequencyPrecision.value
                    uiSettings.customTheme.accent = customTheme.accent
                    uiSettings.customTheme.bgOne = customTheme.bgOne
                    uiSettings.customTheme.bgTwo = customTheme.bgTwo
                    uiSettings.customTheme.borderOne = customTheme.borderOne
                    uiSettings.customTheme.textColor = customTheme.textColor
                    uiSettings.customTheme.textColorSecondary = customTheme.textColorSecondary
                    uiSettings.showOnboarding = showOnboarding.value
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

    function applyThemeMode(): void {
        document.documentElement.classList.remove('high-contrast-dark')
        document.documentElement.classList.remove('high-contrast-light')
        document.documentElement.classList.remove('light-theme')
        document.documentElement.classList.remove('dark-theme')
        document.documentElement.classList.remove('custom-theme')

        // Clear custom theme CSS variables
        document.documentElement.style.removeProperty('--colors-accent')
        document.documentElement.style.removeProperty('--colors-bg-one')
        document.documentElement.style.removeProperty('--colors-bg-two')
        document.documentElement.style.removeProperty('--colors-border-one')
        document.documentElement.style.removeProperty('--colors-text-color')
        document.documentElement.style.removeProperty('--colors-text-color-secondary')

        if (themeMode.value === ThemeMode.SYSTEM) {
            // considered Alpha and doesn't always work as expected:
            // document.documentElement.classList.add('system-theme')
            if (
                window.matchMedia('(prefers-color-scheme: dark) and (prefers-contrast: more)')
                    .matches
            ) {
                document.documentElement.classList.add('high-contrast-dark')
            } else if (
                window.matchMedia('(prefers-color-scheme: light) and (prefers-contrast: more)')
                    .matches
            ) {
                document.documentElement.classList.add('high-contrast-light')
            } else if (window.matchMedia('(prefers-color-scheme: light)').matches) {
                document.documentElement.classList.add('light-theme')
            } else {
                document.documentElement.classList.add('dark-theme')
            }
        } else if (themeMode.value === ThemeMode.HIGH_CONTRAST_DARK) {
            document.documentElement.classList.add('high-contrast-dark')
        } else if (themeMode.value === ThemeMode.HIGH_CONTRAST_LIGHT) {
            document.documentElement.classList.add('high-contrast-light')
        } else if (themeMode.value === ThemeMode.LIGHT) {
            document.documentElement.classList.add('light-theme')
        } else if (themeMode.value === ThemeMode.CUSTOM) {
            document.documentElement.classList.add('custom-theme')
            // Apply custom theme CSS variables
            document.documentElement.style.setProperty('--colors-accent', customTheme.accent)
            document.documentElement.style.setProperty('--colors-bg-one', customTheme.bgOne)
            document.documentElement.style.setProperty('--colors-bg-two', customTheme.bgTwo)
            document.documentElement.style.setProperty('--colors-border-one', customTheme.borderOne)
            document.documentElement.style.setProperty('--colors-text-color', customTheme.textColor)
            document.documentElement.style.setProperty(
                '--colors-text-color-secondary',
                customTheme.textColorSecondary,
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
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('common.toast.thinkPadFanControlApplied'),
                life: 3000,
            })
        }
    }

    console.debug(`Settings Store created`)
    return {
        initializeSettings,
        loadDaemonDeviceSettings,
        predefinedColorOptions,
        profiles,
        functions,
        modes,
        modeActiveCurrent,
        modeActivePrevious,
        modeInEdit,
        allUIDeviceSettings,
        dashboards,
        chartLineScale,
        startInSystemTray,
        closeToSystemTray,
        desktopStartupDelay,
        themeMode,
        uiScale,
        time24,
        collapsedMenuNodeIds,
        collapsedMainMenu,
        hideMenuCollapseIcon,
        menuEntitiesAtBottom,
        mainMenuWidthRem,
        frequencyPrecision,
        customTheme,
        showOnboarding,
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
        loadAlertsAndLogs,
        createAlert,
        updateAlert,
        deleteAlert,
        applyThemeMode,
    }
})
