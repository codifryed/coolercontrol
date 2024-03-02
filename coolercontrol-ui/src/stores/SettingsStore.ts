/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import { defineStore } from 'pinia'
import { Function, FunctionsDTO, Profile, ProfilesDTO } from '@/models/Profile'
import type { Ref } from 'vue'
import { reactive, ref, toRaw, watch } from 'vue'
import {
    type AllDeviceSettings,
    DeviceUISettings,
    DeviceUISettingsDTO,
    SensorAndChannelSettings,
    type SystemOverviewOptions,
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
import { appWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/tauri'
import { ErrorResponse } from '@/models/ErrorResponse'
import { useLayout } from '@/layout/composables/layout'
import { CustomSensor } from '@/models/CustomSensor'
import { CreateModeDTO, Mode, ModeOrderDTO, UpdateModeDTO } from '@/models/Mode.ts'

export const useSettingsStore = defineStore('settings', () => {
    const toast = useToast()

    const deviceStore = useDeviceStore() // using another store internally in this way seems ok, as long as we don't have a circular dependency

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

    const modeActive: Ref<UID | undefined> = ref()

    const modeInEdit: Ref<UID | undefined> = ref()

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

    const systemOverviewOptions: SystemOverviewOptions = reactive({
        selectedTimeRange: { name: '1 min', seconds: 60 },
        selectedChartType: 'TimeChart',
        temp: true,
        duty: true,
        load: true,
        rpm: false,
        timeChartLineScale: 1.5,
    })
    const startInSystemTray: Ref<boolean> = ref(false)
    const closeToSystemTray: Ref<boolean> = ref(false)
    const displayHiddenItems: Ref<boolean> = ref(true)
    const darkMode: Ref<boolean> = ref(true)
    const uiScale: Ref<number> = ref(100)
    const menuMode: Ref<string> = ref('static')
    const time24: Ref<boolean> = ref(false)

    /**
     * This is used to help track various updates that should trigger a refresh of data for the sidebar menu.
     * Currently used to watch for changes indirectly.
     */
    function sidebarMenuUpdate(): void {
        console.debug('Sidebar Menu Update Triggered')
    }

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
        if (uiSettings.systemOverviewOptions != null) {
            systemOverviewOptions.selectedTimeRange =
                uiSettings.systemOverviewOptions.selectedTimeRange
            systemOverviewOptions.selectedChartType =
                uiSettings.systemOverviewOptions.selectedChartType
            systemOverviewOptions.temp = uiSettings.systemOverviewOptions.temp ?? true
            systemOverviewOptions.duty = uiSettings.systemOverviewOptions.duty ?? true
            systemOverviewOptions.load = uiSettings.systemOverviewOptions.load ?? true
            systemOverviewOptions.rpm = uiSettings.systemOverviewOptions.rpm ?? false
            systemOverviewOptions.timeChartLineScale =
                uiSettings.systemOverviewOptions.timeChartLineScale ?? 1.5
        }
        startInSystemTray.value = uiSettings.startInSystemTray
        closeToSystemTray.value = uiSettings.closeToSystemTray
        displayHiddenItems.value = uiSettings.displayHiddenItems
        darkMode.value = uiSettings.darkMode
        uiScale.value = uiSettings.uiScale
        menuMode.value = uiSettings.menuMode
        time24.value = uiSettings.time24
        const layout = useLayout()
        layout.changeThemeSettings(uiSettings.darkMode)
        layout.setScale(uiSettings.uiScale)
        layout.layoutConfig.menuMode.value = uiSettings.menuMode
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
                    } else {
                        // DEPRECATED - backwards compatibility for old hwmon channel names
                        //  - introduced in 1.0.2 - to be removed later
                        for (const device of allDevices) {
                            if (device.uid !== uid) {
                                continue
                            }
                            for (const temp of device.status.temps) {
                                if (
                                    name.toLowerCase().replace(/_/g, ' ') ===
                                    temp.frontend_name.toLowerCase().replace(/_/g, ' ')
                                ) {
                                    deviceSettings.sensorsAndChannels.set(
                                        temp.name,
                                        sensorAndChannelSettings,
                                    )
                                    break
                                }
                            }
                            if (device.info != null) {
                                for (const [channelName, channelInfo] of device.info!.channels) {
                                    if (
                                        name.toLowerCase().replace(/_/g, ' ') ===
                                        channelInfo.label?.toLowerCase().replace(/_/g, ' ')
                                    ) {
                                        deviceSettings.sensorsAndChannels.set(
                                            channelName,
                                            sensorAndChannelSettings,
                                        )
                                        break
                                    }
                                }
                            }
                        }
                    }
                }
                allUIDeviceSettings.value.set(uid, deviceSettings)
            }
        }

        setDefaultSensorAndChannelColors(allDevices, allUIDeviceSettings.value)
        setDisplayNames(allDevices, allUIDeviceSettings.value)
        await loadDaemonDeviceSettings()
        await loadCCAllDeviceSettings()

        await loadFunctions()
        await loadProfiles()
        await loadModes()
        await getActiveMode()

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
                        settings.sensorsAndChannels.get(channelStatus.name)!.displayName =
                            channelStatus.name
                    }
                }
                for (const tempStatus of device.status.temps) {
                    // in the future the temp label will be like the above, instead of status model
                    settings.sensorsAndChannels.get(tempStatus.name)!.displayName =
                        tempStatus.frontend_name
                }
            }
            if (device.info != null) {
                for (const [channelName, channelInfo] of device.info.channels.entries()) {
                    if (channelInfo.speed_options != null) {
                        settings.sensorsAndChannels.get(channelName)!.displayName =
                            channelInfo.label != null
                                ? channelInfo.label
                                : deviceStore.toTitleCase(channelName)
                    } else if (channelInfo.lighting_modes.length > 0) {
                        settings.sensorsAndChannels.get(channelName)!.displayName =
                            deviceStore.toTitleCase(channelName)
                    } else if (channelInfo.lcd_modes.length > 0) {
                        settings.sensorsAndChannels.get(channelName)!.displayName =
                            channelName.toUpperCase()
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

    async function saveFunction(functionUID: UID): Promise<void> {
        console.debug('Saving Function')
        const fun_to_save = functions.value.find((fun) => fun.uid === functionUID)
        if (fun_to_save == null) {
            console.error('Function to save not found: ' + functionUID)
            return
        }
        await deviceStore.daemonClient.saveFunction(fun_to_save)
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

    async function saveProfile(profileUID: UID): Promise<void> {
        console.debug('Saving Profile')
        const profile_to_save = profiles.value.find((profile) => profile.uid === profileUID)
        if (profile_to_save == null) {
            console.error('Profile to save not found: ' + profileUID)
            return
        }
        await deviceStore.daemonClient.saveProfile(profile_to_save)
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
        await loadDaemonDeviceSettings()
    }

    async function loadModes(): Promise<void> {
        console.debug('Loading Modes')
        const modesDTO = await deviceStore.daemonClient.getModes()
        modes.value.length = 0
        modes.value = modesDTO.modes
    }

    async function saveModeOrder(): Promise<void> {
        console.debug('Saving Mode Order')
        const modeOrderDTO = new ModeOrderDTO()
        modeOrderDTO.mode_uids = modes.value.map((mode) => mode.uid)
        await deviceStore.daemonClient.saveModesOrder(modeOrderDTO)
    }

    async function createMode(name: string): Promise<void> {
        console.debug('Creating Mode')
        const createModeDTO = new CreateModeDTO(name)
        const response = await deviceStore.daemonClient.createMode(createModeDTO)
        if (response instanceof Mode) {
            modes.value.push(response)
            await getActiveMode() // deactivate if this mode was active
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Mode successfully created',
                life: 3000,
            })
        } else {
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
        }
    }

    async function updateModeName(modeUID: UID, newName: string): Promise<boolean> {
        console.debug('Updating Mode')
        const updateModeDTO = new UpdateModeDTO(modeUID, newName)
        const response = await deviceStore.daemonClient.updateMode(updateModeDTO)
        if (response instanceof ErrorResponse) {
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
            return false
        } else {
            const mode = modes.value.find((mode) => mode.uid === modeUID)
            if (mode != null) {
                mode.name = newName
            }
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Mode successfully updated',
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
            await getActiveMode() // deactivate if this mode was active
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Mode successfully updated with current settings',
                life: 3000,
            })
            return true
        } else {
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
            return false
        }
    }

    async function deleteMode(modeUID: UID): Promise<void> {
        console.debug('Deleting Mode')
        const response = await deviceStore.daemonClient.deleteMode(modeUID)
        if (response instanceof ErrorResponse) {
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
        } else {
            const index = modes.value.findIndex((mode) => mode.uid === modeUID)
            if (index > -1) {
                modes.value.splice(index, 1)
            }
            await getActiveMode() // clears active mode if it was deleted
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Mode successfully Deleted',
                life: 3000,
            })
        }
    }

    async function getActiveMode(): Promise<void> {
        console.debug('Getting Active Mode')
        modeActive.value = await deviceStore.daemonClient.getActiveModeUID()
    }

    async function activateMode(modeUID: UID): Promise<boolean> {
        console.debug('Activating Mode')
        const response = await deviceStore.daemonClient.activateMode(modeUID)
        if (response instanceof ErrorResponse) {
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
            return false
        } else {
            modeActive.value = modeUID
            await loadDaemonDeviceSettings() // need to reload all settings after applying mode
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Mode successfully Activated',
                life: 3000,
            })
            return true
        }
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
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
        }
    }

    /**
     * The function saves a custom sensor by calling a method from the deviceStore daemon client.
     * @param {CustomSensor} newCustomSensor - The parameter `newCustomSensor` is of type
     * `CustomSensor`.
     * @returns a Promise<boolean>.
     */
    async function saveCustomSensor(newCustomSensor: CustomSensor): Promise<void> {
        console.debug('Saving Custom Sensor')
        const response = await deviceStore.daemonClient.saveCustomSensor(newCustomSensor)
        if (response == null) {
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Custom Sensor Saved and Refreshing UI...',
                life: 3000,
            })
            await deviceStore.waitAndReload()
        } else {
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
        }
    }

    /**
     * The function `updateCustomSensor` updates a custom sensor and returns a boolean indicating if
     * the update was successful.
     * @param {CustomSensor} customSensor - The customSensor parameter is an object that represents a
     * custom sensor.
     * @returns a Promise<boolean>.
     */
    async function updateCustomSensor(customSensor: CustomSensor): Promise<void> {
        console.debug('Updating Custom Sensor')
        const response = await deviceStore.daemonClient.updateCustomSensor(customSensor)
        if (response == null) {
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Custom Sensor successfully updated and Refreshing UI...',
                life: 3000,
            })
            await deviceStore.waitAndReload()
        } else {
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
        }
    }

    /**
     * The function `deleteCustomSensor` is an asynchronous function that deletes a custom sensor
     * and refreshed the UI if successful.
     * @param {UID} deviceUID - The deviceUID parameter is the unique identifier of the custom
     * sensors device. Used to remove any associated user UI settings as well.
     * @param {string} customSensorID - The `customSensorID` parameter is a string that represents
     * the unique identifier of the custom sensor that you want to delete.
     */
    async function deleteCustomSensor(deviceUID: UID, customSensorID: string): Promise<void> {
        console.debug('Deleting Custom Sensor')
        const response = await deviceStore.daemonClient.deleteCustomSensor(customSensorID)
        if (response == null) {
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Custom Sensor successfully deleted and Refreshing UI...',
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
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
        }
    }

    /**
     * This needs to be called after everything is initialized and setup, then we can sync all UI settings automatically.
     */
    async function startWatchingToSaveChanges() {
        watch(
            [
                allUIDeviceSettings.value,
                systemOverviewOptions,
                startInSystemTray,
                closeToSystemTray,
                displayHiddenItems,
                darkMode,
                uiScale,
                menuMode,
                time24,
            ],
            async () => {
                console.debug('Saving UI Settings')
                const uiSettings = new UISettingsDTO()
                for (const [uid, deviceSettings] of allUIDeviceSettings.value) {
                    uiSettings.devices?.push(toRaw(uid))
                    const deviceSettingsDto = new DeviceUISettingsDTO()
                    deviceSettingsDto.menuCollapsed = deviceSettings.menuCollapsed
                    deviceSettingsDto.userName = deviceSettings.userName
                    deviceSettings.sensorsAndChannels.forEach((sensorAndChannelSettings, name) => {
                        deviceSettingsDto.names.push(name)
                        deviceSettingsDto.sensorAndChannelSettings.push(sensorAndChannelSettings)
                    })
                    uiSettings.deviceSettings?.push(deviceSettingsDto)
                }
                uiSettings.systemOverviewOptions = systemOverviewOptions
                uiSettings.startInSystemTray = startInSystemTray.value
                if (deviceStore.isTauriApp()) {
                    if (startInSystemTray.value) {
                        await invoke('start_in_tray_enable')
                    } else {
                        await invoke('start_in_tray_disable')
                    }
                }
                uiSettings.closeToSystemTray = closeToSystemTray.value
                uiSettings.displayHiddenItems = displayHiddenItems.value
                uiSettings.darkMode = darkMode.value
                uiSettings.uiScale = uiScale.value
                uiSettings.menuMode = menuMode.value
                uiSettings.time24 = time24.value
                await deviceStore.daemonClient.saveUISettings(uiSettings)
            },
        )

        watch(ccSettings.value, async () => {
            console.debug('Saving CC Settings')
            await deviceStore.daemonClient.saveCCSettings(ccSettings.value)
        })

        if (deviceStore.isTauriApp()) {
            await appWindow.onCloseRequested(async (event) => {
                if (closeToSystemTray.value) {
                    event.preventDefault()
                    await appWindow.hide()
                }
            })
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
                summary: 'Success',
                detail: 'Settings successfully updated and applied to the device',
                life: 3000,
            })
        } else {
            const message =
                errorMsg != null
                    ? errorMsg
                    : 'There was an error when attempting to apply these settings'
            toast.add({ severity: 'error', summary: 'Error', detail: message, life: 4000 })
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
            toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 4000 })
        } else {
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'ThinkPad Fan Control successfully applied',
                life: 3000,
            })
        }
    }

    console.debug(`Settings Store created`)
    return {
        initializeSettings,
        predefinedColorOptions,
        profiles,
        functions,
        modes,
        modeActive,
        modeInEdit,
        allUIDeviceSettings,
        sidebarMenuUpdate,
        systemOverviewOptions,
        startInSystemTray,
        closeToSystemTray,
        displayHiddenItems,
        darkMode,
        uiScale,
        menuMode,
        time24,
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
        updateMode: updateModeName,
        updateModeSettings,
        deleteMode,
        getActiveMode,
        activateMode,
        getCustomSensor,
        saveCustomSensor,
        updateCustomSensor,
        deleteCustomSensor,
    }
})
