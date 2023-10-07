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

import {defineStore} from "pinia";
import {Profile} from "@/models/Profile";
import type {Ref} from "vue";
import {ref} from "vue";
import {DeviceSettings, SensorAndChannelSettings} from "@/models/UISettings";
import type {UID} from "@/models/Device";
import {Device} from "@/models/Device";
import setDefaultSensorAndChannelColors from "@/stores/DeviceColorCreator";

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

      const allDeviceSettings: Ref<Map<UID, DeviceSettings>> = ref(new Map<UID, DeviceSettings>())

      function sidebarMenuUpdate(): void {
        // this is used to help track various updates that should trigger a refresh of data for the sidebar menu.
      }

      async function initializeSettings(allDevicesIter: IterableIterator<Device>): Promise<void> {
        const allDevices = [...allDevicesIter]
        for (const device of allDevices) {
          const deviceSettings = new DeviceSettings()
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
                settings.icon = 'pi-minus'
                deviceSettings.sensorsAndChannels.setValue(channelName, settings)
              } else if (channelInfo.lcd_modes.length > 0) {
                const settings = new SensorAndChannelSettings()
                settings.icon = 'pi-minus'
                deviceSettings.sensorsAndChannels.setValue(channelName, settings)
              }
            }
          }
          allDeviceSettings.value.set(device.uid, deviceSettings)
        }

        setDefaultSensorAndChannelColors(allDevices, allDeviceSettings.value)
      }

      console.debug(`Settings Store created`)
      return {initializeSettings, predefinedColorOptions, profiles, allDeviceSettings, sidebarMenuUpdate}
    })
