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

import {defineStore} from "pinia"
import {Function, FunctionsDTO, Profile, ProfilesDTO} from "@/models/Profile"
import type {Ref} from "vue"
import {reactive, ref, toRaw, watch} from "vue"
import {
  type AllDeviceSettings,
  DeviceUISettings,
  DeviceUISettingsDTO,
  SensorAndChannelSettings,
  type SystemOverviewOptions,
  UISettingsDTO
} from "@/models/UISettings"
import type {UID} from "@/models/Device"
import {Device} from "@/models/Device"
import setDefaultSensorAndChannelColors from "@/stores/DeviceColorCreator"
import {useDeviceStore} from "@/stores/DeviceStore"
import type {AllDaemonDeviceSettings} from "@/models/DaemonSettings"
import {
  DaemonDeviceSettings,
  DeviceSettingReadDTO,
  DeviceSettingWriteLcdDTO,
  DeviceSettingWriteLightingDTO,
  DeviceSettingWriteManualDTO,
  DeviceSettingWriteProfileDTO,
  DeviceSettingWritePWMModeDTO,
} from "@/models/DaemonSettings"
import {useToast} from "primevue/usetoast"
import {CoolerControlDeviceSettingsDTO, CoolerControlSettingsDTO} from "@/models/CCSettings"
import {appWindow} from "@tauri-apps/api/window"
import {ErrorResponse} from "@/models/ErrorResponse";
import {useLayout} from "@/layout/composables/layout";

export const useSettingsStore =
    defineStore('settings', () => {

      const toast = useToast()

      const deviceStore = useDeviceStore() // using another store internally in this way seems ok, as long as we don't have a circular dependency

      const predefinedColorOptions: Ref<Array<string>> = ref([ // todo: used color history
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

      const allUIDeviceSettings: Ref<AllDeviceSettings> = ref(new Map<UID, DeviceUISettings>())

      const allDaemonDeviceSettings: Ref<AllDaemonDeviceSettings> = ref(new Map<UID, DaemonDeviceSettings>())

      const ccSettings: Ref<CoolerControlSettingsDTO> = ref(new CoolerControlSettingsDTO())

      const ccDeviceSettings: Ref<Map<UID, CoolerControlDeviceSettingsDTO>> = ref(new Map<UID, CoolerControlDeviceSettingsDTO>())
      const ccBlacklistedDevices: Ref<Map<UID, CoolerControlDeviceSettingsDTO>> = ref(new Map<UID, CoolerControlDeviceSettingsDTO>())

      const thinkPadFanControlEnabled: Ref<boolean> = ref(false)

      const systemOverviewOptions: SystemOverviewOptions = reactive({
        selectedTimeRange: {name: '1 min', seconds: 60},
        selectedChartType: 'TimeChart',
      })
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
            deviceSettings.sensorsAndChannels.setValue(temp.name, new SensorAndChannelSettings())
          }
          for (const channel of device.status.channels) { // This gives us both "load" and "speed" channels
            deviceSettings.sensorsAndChannels.setValue(channel.name, new SensorAndChannelSettings())
          }
          if (device.info != null) {
            if (device.info.thinkpad_fan_control != null) {
              thinkPadFanControlEnabled.value = device.info.thinkpad_fan_control
            }
            for (const [channelName, channelInfo] of device.info.channels.entries()) {
              if (channelInfo.lighting_modes.length > 0) {
                const settings = new SensorAndChannelSettings()
                deviceSettings.sensorsAndChannels.setValue(channelName, settings)
              } else if (channelInfo.lcd_modes.length > 0) {
                const settings = new SensorAndChannelSettings()
                deviceSettings.sensorsAndChannels.setValue(channelName, settings)
              }
            }
          }
          allUIDeviceSettings.value.set(device.uid, deviceSettings)
        }

        // load settings from persisted settings, overwriting those that are set
        const uiSettings = await deviceStore.daemonClient.loadUISettings()
        if (uiSettings.systemOverviewOptions != null) {
          systemOverviewOptions.selectedTimeRange = uiSettings.systemOverviewOptions.selectedTimeRange
          systemOverviewOptions.selectedChartType = uiSettings.systemOverviewOptions.selectedChartType
        }
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
        if (uiSettings.devices != null && uiSettings.deviceSettings != null
            && uiSettings.devices.length === uiSettings.deviceSettings.length) {
          for (const [i1, uid] of uiSettings.devices.entries()) {
            const deviceSettingsDto = uiSettings.deviceSettings[i1]
            const deviceSettings = new DeviceUISettings()
            deviceSettings.menuCollapsed = deviceSettingsDto.menuCollapsed
            deviceSettings.userName = deviceSettingsDto.userName
            if (deviceSettingsDto.names.length !== deviceSettingsDto.sensorAndChannelSettings.length) {
              continue
            }
            for (const [i2, name] of deviceSettingsDto.names.entries()) {
              deviceSettings.sensorsAndChannels.setValue(name, deviceSettingsDto.sensorAndChannelSettings[i2])
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

        await startWatchingToSaveChanges()
      }

      async function loadCCSettings(): Promise<void> {
        ccSettings.value = await deviceStore.daemonClient.loadCCSettings()
      }

      function setDisplayNames(devices: Array<Device>, deviceSettings: Map<UID, DeviceUISettings>): void {
        for (const device of devices) {
          const settings = deviceSettings.get(device.uid)!
          // Default display name takes the model name if it's available, before the driver name (HWMon especially):
          settings.displayName = device.info?.model != null && device.info.model.length > 0
              ? device.info.model
              : device.nameShort
          if (device.status_history.length) {
            for (const channelStatus of device.status.channels) {
              const isFanOrPumpChannel = channelStatus.name.includes('fan') || channelStatus.name.includes('pump')
              settings.sensorsAndChannels.getValue(channelStatus.name).displayName =
                  isFanOrPumpChannel ? deviceStore.toTitleCase(channelStatus.name) : channelStatus.name
            }
            for (const tempStatus of device.status.temps) {
              settings.sensorsAndChannels.getValue(tempStatus.name).displayName = tempStatus.frontend_name
            }
          }
          if (device.info != null) {
            for (const [channelName, channelInfo] of device.info.channels.entries()) {
              if (channelInfo.lighting_modes.length > 0) {
                settings.sensorsAndChannels.getValue(channelName).displayName = deviceStore.toTitleCase(channelName)
              } else if (channelInfo.lcd_modes.length > 0) {
                settings.sensorsAndChannels.getValue(channelName).displayName = channelName.toUpperCase()
              }
            }
          }
        }
      }

      async function loadDaemonDeviceSettings(deviceUID: string | undefined = undefined): Promise<void> {
        // allDevices() is used to handle cases where a device may be hidden and no longer available
        for (const device of deviceStore.allDevices()) { // we could load these in parallel, but it's anyway really fast
          if (deviceUID != null && device.uid !== deviceUID) {
            continue
          }
          const deviceSettingsDTO = await deviceStore.daemonClient.loadDeviceSettings(device.uid)
          const deviceSettings = new DaemonDeviceSettings()
          deviceSettingsDTO.settings.forEach(
              (setting: DeviceSettingReadDTO) => deviceSettings.settings.set(setting.channel_name, setting)
          )
          allDaemonDeviceSettings.value.set(device.uid, deviceSettings)
        }
      }

      async function loadCCAllDeviceSettings(): Promise<void> {
        for (const deviceSetting of (await deviceStore.daemonClient.loadCCAllDeviceSettings()).devices) {
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
          throw new Error("Default Function not present in daemon Response. We should not continue.")
        }
        functions.value.length = 0
        functions.value = functionsDTO.functions
      }

      /**
       * Saves the Functions order ONLY to the daemon.
       */
      async function saveFunctionsOrder(): Promise<void> {
        console.debug("Saving Functions Order")
        const functionsDTO = new FunctionsDTO()
        functionsDTO.functions = functions.value
        await deviceStore.daemonClient.saveFunctionsOrder(functionsDTO)
      }

      async function saveFunction(functionUID: UID): Promise<void> {
        console.debug("Saving Function")
        const fun_to_save = functions.value.find(fun => fun.uid === functionUID)
        if (fun_to_save == null) {
          console.error("Function to save not found: " + functionUID)
          return
        }
        await deviceStore.daemonClient.saveFunction(fun_to_save)
      }

      async function updateFunction(functionUID: UID): Promise<boolean> {
        console.debug("Updating Function")
        const fun_to_update = functions.value.find(fun => fun.uid === functionUID)
        if (fun_to_update == null) {
          console.error("Function to update not found: " + functionUID)
          return false
        }
        return await deviceStore.daemonClient.updateFunction(fun_to_update)
      }

      async function deleteFunction(functionUID: UID): Promise<void> {
        console.debug("Deleting Function")
        await deviceStore.daemonClient.deleteFunction(functionUID)
        await loadProfiles() // need to reload any changes to profiles from the Function removal
      }

      /**
       * Loads all the Profiles from the daemon. The default Profile must be included.
       */
      async function loadProfiles(): Promise<void> {
        const profilesDTO = await deviceStore.daemonClient.loadProfiles()
        if (profilesDTO.profiles.find((profile: Profile) => profile.uid === '0') == null) {
          throw new Error("Default Profile not present in daemon Response. We should not continue.")
        }
        profiles.value.length = 0
        profiles.value = profilesDTO.profiles
      }

      /**
       * Saves the Profiles Order ONLY to the daemon.
       */
      async function saveProfilesOrder(): Promise<void> {
        console.debug("Saving Profiles Order")
        const profilesDTO = new ProfilesDTO()
        profilesDTO.profiles = profiles.value
        await deviceStore.daemonClient.saveProfilesOrder(profilesDTO)
      }

      async function saveProfile(profileUID: UID): Promise<void> {
        console.debug("Saving Profile")
        const profile_to_save = profiles.value.find(profile => profile.uid === profileUID)
        if (profile_to_save == null) {
          console.error("Profile to save not found: " + profileUID)
          return
        }
        await deviceStore.daemonClient.saveProfile(profile_to_save)
      }

      async function updateProfile(profileUID: UID): Promise<boolean> {
        console.debug("Updating Profile")
        const profile_to_update = profiles.value.find(profile => profile.uid === profileUID)
        if (profile_to_update == null) {
          console.error("Profile to update not found: " + profileUID)
          return false
        }
        return await deviceStore.daemonClient.updateProfile(profile_to_update)
      }

      async function deleteProfile(profileUID: UID): Promise<void> {
        console.debug("Deleting Profile")
        await deviceStore.daemonClient.deleteProfile(profileUID)
        await loadDaemonDeviceSettings()
      }

      /**
       * This needs to be called after everything is initialized and setup, then we can sync all UI settings automatically.
       */
      async function startWatchingToSaveChanges() {
        watch([
          allUIDeviceSettings.value,
          systemOverviewOptions,
          closeToSystemTray,
          displayHiddenItems,
          darkMode,
          uiScale,
          menuMode,
          time24
        ], async () => {
          console.debug("Saving UI Settings")
          const uiSettings = new UISettingsDTO()
          for (const [uid, deviceSettings] of allUIDeviceSettings.value) {
            uiSettings.devices?.push(toRaw(uid))
            const deviceSettingsDto = new DeviceUISettingsDTO()
            deviceSettingsDto.menuCollapsed = deviceSettings.menuCollapsed
            deviceSettingsDto.userName = deviceSettings.userName
            deviceSettings.sensorsAndChannels.forEach((name, sensorAndChannelSettings) => {
              deviceSettingsDto.names.push(name)
              deviceSettingsDto.sensorAndChannelSettings.push(sensorAndChannelSettings)
            })
            uiSettings.deviceSettings?.push(deviceSettingsDto)
          }
          uiSettings.systemOverviewOptions = systemOverviewOptions
          uiSettings.closeToSystemTray = closeToSystemTray.value
          uiSettings.displayHiddenItems = displayHiddenItems.value
          uiSettings.darkMode = darkMode.value
          uiSettings.uiScale = uiScale.value
          uiSettings.menuMode = menuMode.value
          uiSettings.time24 = time24.value
          await deviceStore.daemonClient.saveUISettings(uiSettings)
        })

        watch(ccSettings.value, async () => {
          console.debug("Saving CC Settings")
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
          errorMsg: string | undefined = undefined
      ): Promise<void> {
        if (successful) {
          await loadDaemonDeviceSettings(deviceUID)
          toast.add({
            severity: 'success',
            summary: 'Success',
            detail: 'Settings successfully updated and applied to the device',
            life: 3000
          })
        } else {
          const message = errorMsg != null ? errorMsg : 'There was an error when attempting to apply these settings'
          toast.add({severity: 'error', summary: 'Error', detail: message, life: 4000})
        }
        console.debug('Daemon Settings Saved')
      }

      async function saveDaemonDeviceSettingManual(
          deviceUID: UID,
          channelName: string,
          setting: DeviceSettingWriteManualDTO
      ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingManual(deviceUID, channelName, setting)
        await handleSaveDeviceSettingResponse(deviceUID, successful)
      }

      async function saveDaemonDeviceSettingProfile(
          deviceUID: UID,
          channelName: string,
          setting: DeviceSettingWriteProfileDTO
      ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingProfile(deviceUID, channelName, setting)
        await handleSaveDeviceSettingResponse(deviceUID, successful)
      }

      async function saveDaemonDeviceSettingLcd(
          deviceUID: UID,
          channelName: string,
          setting: DeviceSettingWriteLcdDTO
      ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingLcd(deviceUID, channelName, setting)
        await handleSaveDeviceSettingResponse(deviceUID, successful)
      }

      async function saveDaemonDeviceSettingLcdImages(
          deviceUID: UID,
          channelName: string,
          setting: DeviceSettingWriteLcdDTO,
          files: Array<File>,
      ): Promise<void> {
        const response = await deviceStore.daemonClient.saveDeviceSettingLcdImages(deviceUID, channelName, setting, files)
        const successful = response === undefined
        await handleSaveDeviceSettingResponse(deviceUID, successful, response?.error)
      }

      async function saveDaemonDeviceSettingLighting(
          deviceUID: UID,
          channelName: string,
          setting: DeviceSettingWriteLightingDTO
      ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingLighting(deviceUID, channelName, setting)
        await handleSaveDeviceSettingResponse(deviceUID, successful)
      }

      async function saveDaemonDeviceSettingPWM(
          deviceUID: UID,
          channelName: string,
          setting: DeviceSettingWritePWMModeDTO
      ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingPWM(deviceUID, channelName, setting)
        await handleSaveDeviceSettingResponse(deviceUID, successful)
      }

      async function saveDaemonDeviceSettingReset(
          deviceUID: UID,
          channelName: string,
      ): Promise<void> {
        const successful = await deviceStore.daemonClient.saveDeviceSettingReset(deviceUID, channelName)
        await handleSaveDeviceSettingResponse(deviceUID, successful)
      }

      async function applyThinkPadFanControl(
          enable: boolean
      ): Promise<void> {
        const response: undefined | ErrorResponse = await deviceStore.daemonClient.thinkPadFanControl(enable)
        if (response instanceof ErrorResponse) {
          toast.add({severity: 'error', summary: 'Error', detail: response.error, life: 4000})
        } else {
          toast.add({severity: 'success', summary: 'Success', detail: 'ThinkPad Fan Control successfully applied', life: 3000})
        }
      }

      console.debug(`Settings Store created`)
      return {
        initializeSettings, predefinedColorOptions, profiles, functions, allUIDeviceSettings, sidebarMenuUpdate,
        systemOverviewOptions,
        closeToSystemTray,
        displayHiddenItems,
        darkMode,
        uiScale,
        menuMode,
        time24,
        allDaemonDeviceSettings,
        ccSettings, ccDeviceSettings, ccBlacklistedDevices,
        thinkPadFanControlEnabled, applyThinkPadFanControl,
        saveDaemonDeviceSettingManual, saveDaemonDeviceSettingProfile,
        saveDaemonDeviceSettingLcd, saveDaemonDeviceSettingLcdImages,
        saveDaemonDeviceSettingLighting, saveDaemonDeviceSettingPWM, saveDaemonDeviceSettingReset,
        saveFunctionsOrder, saveFunction, updateFunction, deleteFunction,
        saveProfilesOrder, saveProfile, updateProfile, deleteProfile,
      }
    })
