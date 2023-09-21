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
  if (device.info != null) {
    for (const [channelName, channelInfo] of device.info.channels.entries()) {
      if (channelInfo.speed_options) {
        const foundMatchingSensors: ChannelStatus | undefined = device.status.channels
            .find((channelStatus) => channelStatus.name === channelName)
        const duty = foundMatchingSensors?.duty ?? 0
        const rpm = foundMatchingSensors?.rpm ?? 0
        // @ts-ignore
        deviceItem.items.push({
          label: channelName,
          icon: 'pi pi-fw pi-minus',
          // icon: icon,
          iconStyle: `color: ${device.colors.getValue(channelName)};`,
          to: {name: 'device-speed', params: {deviceId: device.uid, name: channelName}},
          duty: duty.toFixed(1),
          rpm: rpm,
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
        });
      } else if (channelInfo.lighting_modes.length > 0) {
        // @ts-ignore
        deviceItem.items.push({
          label: channelName,
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
          label: channelName,
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
model.value.push(
    {
      label: '',
      items: [
        {
          label: 'Wiki',
          icon: 'pi pi-fw pi-question-circle',
          // @ts-ignore
          url: 'https://gitlab.com/coolercontrol/coolercontrol/-/wikis/home',
          target: '_blank'
        },
        {
          label: 'Project Page',
          icon: 'pi pi-fw pi-github',
          // @ts-ignore
          url: 'https://gitlab.com/coolercontrol/coolercontrol/',
          target: '_blank'
        }
      ]
    }
)
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
