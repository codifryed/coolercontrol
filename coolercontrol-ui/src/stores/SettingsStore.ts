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
import {Profile} from "@/models/Profile"
import type {Ref} from "vue"
import {reactive, ref, toRaw, watch} from "vue"
import {
  type AllDeviceSettings,
  DeviceUISettings, DeviceUISettingsDTO,
  SensorAndChannelSettings,
  type SystemOverviewOptions,
  UISettingsDTO
} from "@/models/UISettings"
import type {UID} from "@/models/Device"
import {Device} from "@/models/Device"
import setDefaultSensorAndChannelColors from "@/stores/DeviceColorCreator"
import {useDeviceStore} from "@/stores/DeviceStore"

export const useSettingsStore =
    defineStore('settings', () => {

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
      const profiles: Ref<Array<Profile>> = ref([Profile.createDefault()])

      const allDeviceSettings: Ref<AllDeviceSettings> = ref(new Map<UID, DeviceUISettings>())


      const systemOverviewOptions: SystemOverviewOptions = reactive({
        selectedTimeRange: {name: '1 min', seconds: 60},
        selectedChartType: 'TimeChart',
      })

      function sidebarMenuUpdate(): void {
        // this is used to help track various updates that should trigger a refresh of data for the sidebar menu.
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
          allDeviceSettings.value.set(device.uid, deviceSettings)
        }

        setDefaultSensorAndChannelColors(allDevices, allDeviceSettings.value)

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
            if (deviceSettingsDto.names.length !== deviceSettingsDto.sensorAndChannelSettings.length) {
              continue
            }
            for (const [i2, name] of deviceSettingsDto.names.entries()) {
              deviceSettings.sensorsAndChannels.setValue(name, deviceSettingsDto.sensorAndChannelSettings[i2])
            }
            allDeviceSettings.value.set(uid, deviceSettings)
          }
        }

        startWatchingToSaveChanges()
      }

      /**
       * This needs to be called after everything is initialized and setup, then we can sync all UI settings automatically.
       */
      function startWatchingToSaveChanges() {
        watch(profiles, () => {
          // todo: save profiles to their own endpoint and own place in the config file
        })

        watch([allDeviceSettings.value, systemOverviewOptions], async () => {
          console.debug("Saving UI Settings")
          const deviceStore = useDeviceStore()
          const uiSettings = new UISettingsDTO()
          for (const [uid, deviceSettings] of allDeviceSettings.value) {
            uiSettings.devices?.push(toRaw(uid))
            const deviceSettingsDto = new DeviceUISettingsDTO()
            deviceSettingsDto.menuCollapsed = deviceSettings.menuCollapsed
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


      console.debug(`Settings Store created`)
      return {
        initializeSettings, predefinedColorOptions, profiles, allDeviceSettings, sidebarMenuUpdate,
        systemOverviewOptions
      }
    })
