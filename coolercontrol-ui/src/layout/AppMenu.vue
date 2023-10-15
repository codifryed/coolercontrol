<script setup lang="ts">
import {ref} from 'vue'

import AppMenuItem from './AppMenuItem.vue'
import {useDeviceStore} from "@/stores/DeviceStore"
import {useSettingsStore} from "@/stores/SettingsStore"
import {
  mdiChartLine, mdiChip,
  mdiLedOn,
  mdiPencilBoxMultipleOutline,
  mdiTelevisionShimmer
} from "@mdi/js"

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
      }
    ]
  },
])
model.value.push(
    {
      label: '',
      items: [
        {
          label: 'Profiles',
          icon: mdiPencilBoxMultipleOutline,
          to: {name: 'profiles'},
        },
        // todo: add 'functions'
        // {
        //   label: 'Functions',
        //   icon: mdiFunctionVariant,
        //   to: {name: 'functions'},
        // }
        // todo: add 'Sensors'
        // {
        //   label: 'Custom Sensors',
        //   icon: mdiFunctionVariant...,
        //   to: {name: 'sensors'},
        // }
      ]
    }
)

const deviceItems = {
  label: '',
  items: [],
}
for (const device of deviceStore.allDevices()) {
  const deviceItem = {
    label: device.nameShort,
    icon: mdiChip,
    deviceUID: device.uid,
    options: [
      {
        label: 'Hide All',
        icon: 'pi pi-fw pi-check',
      },
      // { todo:
      //   label: 'Ignore',
      //   // icon: 'pi pi-fw pi-check',
      // }
    ],
    items: [],
  }
  const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
  for (const temp of device.status.temps) {
    // @ts-ignore
    deviceItem.items.push({
      label: temp.frontend_name,
      name: temp.name,
      color: true,
      to: {name: 'device-temp', params: {deviceId: device.uid, name: temp.name}},
      deviceUID: device.uid,
      temp: temp.temp.toFixed(1),
      options: [
        {
          label: 'Hide',
        },
      ],
    })
  }
  for (const channel of device.status.channels) { // This gives us both "load" and "speed" channels
    const isFanOrPumpChannel = channel.name.includes('fan') || channel.name.includes('pump')
    // @ts-ignore
    deviceItem.items.push({
      label: isFanOrPumpChannel ? deviceStore.toTitleCase(channel.name) : channel.name,
      name: channel.name,
      color: true,
      to: {
        name: isFanOrPumpChannel ? 'device-speed' : 'device-load',
        params: {deviceId: device.uid, name: channel.name}
      },
      deviceUID: device.uid,
      duty: channel.duty?.toFixed(1),
      rpm: channel.rpm,
      options: [
        {
          label: 'Hide',
        },
      ],
    })
  }
  if (device.info != null) {
    for (const [channelName, channelInfo] of device.info.channels.entries()) {
      if (channelInfo.lighting_modes.length > 0) {
        // @ts-ignore
        deviceItem.items.push({
          label: deviceStore.toTitleCase(channelName),
          name: channelName,
          icon: mdiLedOn,
          iconStyle: `color: ${deviceSettings.sensorsAndChannels.getValue(channelName).color};`,
          to: {name: 'device-lighting', params: {deviceId: device.uid, name: channelName}},
          deviceUID: device.uid,
          // options: [
          //   {
          //     label: 'Hide',
          //   },
          // ],
        });
      } else if (channelInfo.lcd_modes.length > 0) {
        // @ts-ignore
        deviceItem.items.push({
          label: channelName.toUpperCase(),
          name: channelName,
          icon: mdiTelevisionShimmer,
          iconStyle: `color: ${deviceSettings.sensorsAndChannels.getValue(channelName).color};`,
          to: {name: 'device-lcd', params: {deviceId: device.uid, name: channelName}},
          deviceUID: device.uid,
          // options: [
          //   {
          //     label: 'Hide',
          //   },
          // ],
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
      <!--      <app-menu-item :item="item" :index="i"></app-menu-item>-->
      <app-menu-item v-if="!item.separator" :item="item" :index="i"></app-menu-item>
      <li v-if="item.separator" class="menu-separator"></li>
    </template>
  </ul>
</template>

<style lang="scss" scoped>
//
</style>
