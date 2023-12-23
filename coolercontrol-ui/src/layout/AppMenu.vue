<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import {ref} from 'vue'

// @ts-ignore
import AppMenuItem from './AppMenuItem.vue'
import {useDeviceStore} from "@/stores/DeviceStore"
import {useSettingsStore} from "@/stores/SettingsStore"
import {
  mdiCarBrakeTemperature,
  mdiChartLine,
  mdiChip,
  mdiLayersTripleOutline,
  mdiLedOn,
  mdiPencilBoxMultipleOutline,
  mdiTelevisionShimmer
} from "@mdi/js"
import { DeviceType } from '@/models/Device'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const model = ref([
  {
    label: '',
    items: [
      {
        label: 'System Overview',
        icon: mdiChartLine,
        to: {name: 'system-overview'}
      },
      {
        label: 'Profiles & Functions',
        icon: mdiPencilBoxMultipleOutline,
        to: {name: 'profiles-functions'},
      },
      // {
      //   label: 'System Profiles',
      //   icon: mdiLayersTripleOutline,
      //   to: {name: 'system-profiles'},
      // },
    ],
  },
])


// Custom Sensors Menu
const customSensorsItems = {
  label: '',
  items: [],
}
for (const device of deviceStore.allDevices()) {
  if (device.type !== DeviceType.CUSTOM_SENSORS) {
    continue;
  }
  const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
  const deviceItem = {
    label: deviceSettings.name,
    icon: mdiCarBrakeTemperature,
    deviceUID: device.uid,
    options: [
      {
        label: 'Add Sensor',
        icon: 'pi pi-fw pi-plus',
      },
      {
        label: 'Rename',
        icon: 'pi pi-fw pi-pencil',
      },
    ],
    items: [],
  }
  for (const temp of device.status.temps) {
    // @ts-ignore
    deviceItem.items.push({
      label: deviceSettings.sensorsAndChannels.getValue(temp.name).name,
      name: temp.name,
      color: true,
      to: {name: 'device-temp', params: {deviceId: device.uid, name: temp.name}},
      deviceUID: device.uid,
      temp: temp.temp.toFixed(1),
      options: [
        {
          label: 'Hide',
        },
        {
          label: 'Rename',
          icon: 'pi pi-fw pi-pencil',
        },
        {
          label: 'Edit',
          icon: 'pi pi-fw pi-wrench',
        },
        {
          label: 'Delete',
          icon: 'pi pi-fw pi-minus',
        },
      ],
    })
  }
  // @ts-ignore
  customSensorsItems.items.push(deviceItem)
}
model.value.push(customSensorsItems)

const deviceItems = {
  label: '',
  items: [],
}
for (const device of deviceStore.allDevices()) {
  if (device.type === DeviceType.CUSTOM_SENSORS) {
    continue; // has it's own dedicated menu above
  }
  const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
  const deviceItem = {
    label: deviceSettings.name,
    icon: mdiChip,
    deviceUID: device.uid,
    options: [
      {
        label: 'Hide All',
        icon: 'pi pi-fw pi-check',
      },
      {
        label: 'Rename',
        icon: 'pi pi-fw pi-pencil',
      },
      {
        label: 'Blacklist',
        icon: 'pi pi-fw pi-lock',
      }
    ],
    items: [],
  }
  for (const temp of device.status.temps) {
    // @ts-ignore
    deviceItem.items.push({
      label: deviceSettings.sensorsAndChannels.getValue(temp.name).name,
      name: temp.name,
      color: true,
      to: {name: 'device-temp', params: {deviceId: device.uid, name: temp.name}},
      deviceUID: device.uid,
      temp: temp.temp.toFixed(1),
      options: [
        {
          label: 'Hide',
        },
        {
          label: 'Rename',
          icon: 'pi pi-fw pi-pencil',
        },
      ],
    })
  }
  for (const channel of device.status.channels) { // This gives us both "load" and "speed" channels
    const isFanOrPumpChannel = channel.name.includes('fan') || channel.name.includes('pump')
    // @ts-ignore
    deviceItem.items.push({
      label: deviceSettings.sensorsAndChannels.getValue(channel.name).name,
      name: channel.name,
      color: true,
      to: {
        name: isFanOrPumpChannel ? 'device-speed' : 'device-load',
        params: {deviceId: device.uid, name: channel.name}
      },
      deviceUID: device.uid,
      duty: channel.duty,
      rpm: channel.rpm,
      options: [
        {
          label: 'Hide',
        },
        {
          label: 'Rename',
          icon: 'pi pi-fw pi-pencil',
        },
      ],
    })
  }
  if (device.info != null) {
    for (const [channelName, channelInfo] of device.info.channels.entries()) {
      if (channelInfo.lighting_modes.length > 0) {
        // @ts-ignore
        deviceItem.items.push({
          label: deviceSettings.sensorsAndChannels.getValue(channelName).name,
          name: channelName,
          icon: mdiLedOn,
          iconStyle: `color: ${deviceSettings.sensorsAndChannels.getValue(channelName).color};`,
          to: {name: 'device-lighting', params: {deviceId: device.uid, name: channelName}},
          deviceUID: device.uid,
          options: [
            {
              label: 'Hide',
            },
            {
              label: 'Rename',
              icon: 'pi pi-fw pi-pencil',
            },
          ],
        });
      } else if (channelInfo.lcd_modes.length > 0) {
        // @ts-ignore
        deviceItem.items.push({
          label: deviceSettings.sensorsAndChannels.getValue(channelName).name,
          name: channelName,
          icon: mdiTelevisionShimmer,
          iconStyle: `color: ${deviceSettings.sensorsAndChannels.getValue(channelName).color};`,
          to: {name: 'device-lcd', params: {deviceId: device.uid, name: channelName}},
          deviceUID: device.uid,
          options: [
            {
              label: 'Hide',
            },
            {
              label: 'Rename',
              icon: 'pi pi-fw pi-pencil',
            },
          ],
        });
      }
    }
  }
  // @ts-ignore
  deviceItems.items.push(deviceItem)
}
model.value.push(deviceItems)
</script>

<template>
  <ul class="layout-menu">
    <template v-for="(item, i) in model" :key="item">
      <app-menu-item :item="item" :index="i"></app-menu-item>
      <!--<app-menu-item v-if="!item.separator" :item="item" :index="i"></app-menu-item>-->
      <!--<li v-if="item.separator" class="menu-separator"></li>-->
    </template>
  </ul>
</template>

<style lang="scss" scoped>
//
</style>
