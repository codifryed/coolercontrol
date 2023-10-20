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
import {Profile, Function, FunctionsDTO, ProfilesDTO} from "@/models/Profile"
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
import {DaemonDeviceSettings, DeviceSettingDTO} from "@/models/DaemonSettings"
import {useToast} from "primevue/usetoast"

export const useSettingsStore =
    defineStore('settings', () => {

      const toast = useToast()

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

      const systemOverviewOptions: SystemOverviewOptions = reactive({
        selectedTimeRange: {name: '1 min', seconds: 60},
        selectedChartType: 'TimeChart',
      })

      /**
       * This is used to help track various updates that should trigger a refresh of data for the sidebar menu.
       * Currently used to watch for changes indirectly.
       */
      function sidebarMenuUpdate(): void {
        console.debug('Sidebar Menu Update Triggered')
      }

      async function initializeSettings(allDevicesIter: IterableIterator<Device>): Promise<void> {
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

        setDefaultSensorAndChannelColors(allDevices, allUIDeviceSettings.value)

        // load settings from persisted settings, overwriting those that are set
        const deviceStore = useDeviceStore()
        const uiSettings = await deviceStore.loadUiSettings()
        if (uiSettings.systemOverviewOptions != null) {
          systemOverviewOptions.selectedTimeRange = uiSettings.systemOverviewOptions.selectedTimeRange
          systemOverviewOptions.selectedChartType = uiSettings.systemOverviewOptions.selectedChartType
        }
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
        setDisplayNames(allDevices, allUIDeviceSettings.value)
        await loadDaemonDeviceSettings()

        await loadFunctions()
        await loadProfiles()

        startWatchingToSaveChanges()
      }

      function setDisplayNames(devices: Array<Device>, deviceSettings: Map<UID, DeviceUISettings>): void {
        const deviceStore = useDeviceStore()
        for (const device of devices) {
          const settings = deviceSettings.get(device.uid)!
          settings.displayName = device.nameShort
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
        const deviceStore = useDeviceStore()
        for (const device of deviceStore.allDevices()) { // we could load these in parallel, but it's anyway really fast
          if (deviceUID != null && device.uid !== deviceUID) {
            continue
          }
          const deviceSettingsDTO = await deviceStore.loadDeviceSettings(device.uid)
          const deviceSettings = new DaemonDeviceSettings()
          deviceSettingsDTO.settings.forEach(
              setting => deviceSettings.settings.set(setting.channel_name, setting)
          )
          allDaemonDeviceSettings.value.set(device.uid, deviceSettings)
        }
      }

      /**
       * Loads all the Functions from the daemon. The default Function must be included.
       * These should be loaded before Profiles, as Profiles reference associated Functions.
       */
      async function loadFunctions(): Promise<void> {
        const deviceStore = useDeviceStore()
        const functionsDTO = await deviceStore.loadFunctions()
        if (functionsDTO.functions.find(fun => fun.uid === '0') == null) {
          throw new Error("Default Function not present in daemon Response. We should not continue.")
        }
        functions.value.length = 0
        functions.value = functionsDTO.functions
      }

      /**
       * Saves all the Functions to the daemon. This is used instead of watching due to the very dynamic nature of
       * live changes in the editor.
       */
      async function saveFunctions(): Promise<void> {
        console.debug("Saving Functions")
        const deviceStore = useDeviceStore()
        const functionsDTO = new FunctionsDTO()
        functionsDTO.functions = functions.value
        await deviceStore.saveFunctions(functionsDTO)
      }

      /**
       * Loads all the Profiles from the daemon. The default Profile must be included.
       */
      async function loadProfiles(): Promise<void> {
        const deviceStore = useDeviceStore()
        const profilesDTO = await deviceStore.loadProfiles()
        if (profilesDTO.profiles.find(profile => profile.uid === '0') == null) {
          throw new Error("Default Profile not present in daemon Response. We should not continue.")
        }
        profiles.value.length = 0
        profiles.value = profilesDTO.profiles
      }

      /**
       * Saves all the Profiles to the daemon. This is used instead of watching due to the very dynamic nature of
       * live changes in the editor.
       */
      async function saveProfiles(): Promise<void> {
        console.debug("Saving Profiles")
        const deviceStore = useDeviceStore()
        const profilesDTO = new ProfilesDTO()
        profilesDTO.profiles = profiles.value
        await deviceStore.saveProfiles(profilesDTO)
      }

      /**
       * This needs to be called after everything is initialized and setup, then we can sync all UI settings automatically.
       */
      function startWatchingToSaveChanges() {
        watch([allUIDeviceSettings.value, systemOverviewOptions], async () => {
          console.debug("Saving UI Settings")
          const deviceStore = useDeviceStore()
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
          await deviceStore.saveUiSettings(uiSettings)
        })
      }

      async function saveDaemonDeviceSetting(deviceUID: UID, deviceSetting: DeviceSettingDTO): Promise<void> {
        const deviceStore = useDeviceStore()
        const successful = await deviceStore.saveDeviceSetting(deviceUID, deviceSetting)
        if (successful) {
          await loadDaemonDeviceSettings(deviceUID)
          toast.add({severity: 'success', summary: 'Success', detail: 'Settings successfully applied', life: 3000})
        } else {
          toast.add({severity: 'error', summary: 'Error', detail: 'Error received when attempting to apply settings', life: 3000})
        }
        console.debug('Daemon Settings Saved')
      }


      console.debug(`Settings Store created`)
      return {
        initializeSettings, predefinedColorOptions, profiles, functions, allUIDeviceSettings, sidebarMenuUpdate,
        systemOverviewOptions, allDaemonDeviceSettings, saveDaemonDeviceSetting, saveFunctions, saveProfiles,
      }
    })
