<script setup lang="ts">
import {ref} from 'vue'

import AppMenuItem from './AppMenuItem.vue'
import {useDeviceStore} from "@/stores/DeviceStore";
import {ChannelStatus} from "@/models/Status";

const deviceStore = useDeviceStore()

const model = ref([
  {
    label: '',
    items: [
      {label: 'System Overview', icon: 'pi pi-fw pi-chart-line', to: {name: 'system-overview'}}
    ]
  },
])
model.value.push(
    {
      label: '',
      items: [
        {
          label: 'Profiles',
          icon: 'pi pi-fw pi-chart-bar',
          to: {name: 'profiles'},
        },
        {
          label: 'Functions',
          icon: 'pi pi-fw pi-calculator',
          to: {name: 'functions'},
        }
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
    icon: 'pi pi-fw pi-server',
    options: [
      {
        label: 'Hide',
        // icon: 'pi pi-fw pi-check',
      },
      {
        label: 'Ignore',
        // icon: 'pi pi-fw pi-check',
      }
    ],
    items: [],
  }
  for (const temp of device.status.temps) {
    // @ts-ignore
    deviceItem.items.push({
      label: temp.frontend_name,
      icon: 'pi pi-fw pi-minus',
      iconStyle: `color: ${device.colors.getValue(temp.name)};`,
      to: {name: 'device-temp', params: {deviceId: device.uid, name: temp.name}},
      temp: temp.temp.toFixed(1),
      options: [
        {
          label: 'Hide',
          // icon: 'pi pi-fw pi-check',
        },
        {
          label: 'Color',
          // icon: 'pi pi-fw pi-check',
        }
      ],
    })
  }
  for (const channel of device.status.channels) { // This gives us both "load" and "speed" channels
    // @ts-ignore
    deviceItem.items.push({
      label: deviceStore.toTitleCase(channel.name),
      icon: 'pi pi-fw pi-minus',
      iconStyle: `color: ${device.colors.getValue(channel.name)};`,
      to: {name: 'device-speed', params: {deviceId: device.uid, name: channel.name}},
      duty: channel.duty?.toFixed(1),
      rpm: channel.rpm,
      options: [
        {
          label: 'Hide',
          // icon: 'pi pi-fw pi-check',
        },
        {
          label: 'Color',
          // icon: 'pi pi-fw pi-check',
        }
      ],
    })
  }
  if (device.info != null) {
    for (const [channelName, channelInfo] of device.info.channels.entries()) {
      if (channelInfo.lighting_modes.length > 0) {
        // @ts-ignore
        deviceItem.items.push({
          label: deviceStore.toTitleCase(channelName),
          icon: 'pi pi-fw pi-minus',
          // icon: icon,
          iconStyle: `color: ${device.colors.getValue(channelName)};`,
          to: {name: 'device-lighting', params: {deviceId: device.uid, name: channelName}},
          options: [
            {
              label: 'Hide',
              // icon: 'pi pi-fw pi-check',
            },
          ],
        });
      } else if (channelInfo.lcd_modes.length > 0) {
        // @ts-ignore
        deviceItem.items.push({
          label: channelName.toUpperCase(),
          icon: 'pi pi-fw pi-minus',
          // icon: icon,
          iconStyle: `color: ${device.colors.getValue(channelName)};`,
          to: {name: 'device-lcd', params: {deviceId: device.uid, name: channelName}},
          options: [
            {
              label: 'Hide',
              // icon: 'pi pi-fw pi-check',
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
      <!--      <app-menu-item :item="item" :index="i"></app-menu-item>-->
      <app-menu-item v-if="!item.separator" :item="item" :index="i"></app-menu-item>
      <li v-if="item.separator" class="menu-separator"></li>
    </template>
  </ul>
</template>

<style lang="scss" scoped>
//
</style>
