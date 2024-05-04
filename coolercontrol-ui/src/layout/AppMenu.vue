<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2024  Guy Boldon and contributors
  -
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  -
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  -
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import { ref } from 'vue'

// @ts-ignore
import AppMenuItem from './AppMenuItem.vue'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import {
    mdiCarBrakeTemperature,
    mdiChartLine,
    mdiChip,
    mdiLayersTripleOutline,
    mdiLedOn,
    mdiPencilBoxMultipleOutline,
    mdiTelevisionShimmer,
} from '@mdi/js'
import { DeviceType } from '@/models/Device'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const model = ref([
    {
        label: '',
        items: [
            {
                label: 'Overview',
                icon: mdiChartLine,
                to: { name: 'system-overview' },
            },
            {
                label: 'Modes',
                icon: mdiLayersTripleOutline,
                to: { name: 'modes' },
            },
            {
                label: 'Profiles & Functions',
                icon: mdiPencilBoxMultipleOutline,
                to: { name: 'profiles-functions' },
            },
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
        continue
    }
    const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
    const deviceItem = {
        label: deviceSettings.name,
        icon: mdiCarBrakeTemperature,
        deviceUID: device.uid,
        customSensors: true, // special menu
        items: [],
    }
    for (const temp of device.status.temps) {
        // @ts-ignore
        deviceItem.items.push({
            label: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
            name: temp.name,
            color: true,
            to: { name: 'device-temp', params: { deviceId: device.uid, name: temp.name } },
            deviceUID: device.uid,
            temp: temp.temp.toFixed(1),
            options: [
                {
                    label: 'Hide',
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

// Device Items need to be added in the oder they should appear in the menu
//  - devices are pre-sorted, but channels are not
const deviceItems = {
    label: '',
    items: [],
}
for (const device of deviceStore.allDevices()) {
    if (device.type === DeviceType.CUSTOM_SENSORS) {
        continue // has its own dedicated menu above
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
            },
        ],
        items: [],
    }
    for (const temp of device.status.temps) {
        // @ts-ignore
        deviceItem.items.push({
            label: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
            name: temp.name,
            color: true,
            to: { name: 'device-temp', params: { deviceId: device.uid, name: temp.name } },
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
    for (const channel of device.status.channels) {
        if (channel.name.toLowerCase().includes('freq')) {
            // @ts-ignore
            deviceItem.items.push({
                label: deviceSettings.sensorsAndChannels.get(channel.name)!.name,
                name: channel.name,
                color: true,
                to: {
                    name: 'device-freq',
                    params: { deviceId: device.uid, name: channel.name },
                },
                deviceUID: device.uid,
                freq: channel.freq,
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
    }
    for (const channel of device.status.channels) {
        if (channel.name.toLowerCase().includes('load')) {
            // @ts-ignore
            deviceItem.items.push({
                label: deviceSettings.sensorsAndChannels.get(channel.name)!.name,
                name: channel.name,
                color: true,
                to: {
                    name: 'device-load',
                    params: { deviceId: device.uid, name: channel.name },
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
    }
    if (device.info != null) {
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.speed_options === null) {
                continue
            }
            // need to get the status data to properly setup the menu item
            let duty: number | undefined = undefined
            let rpm: number | undefined = undefined
            for (const channel of device.status.channels) {
                if (channel.name === channelName) {
                    duty = channel.duty
                    rpm = channel.rpm
                    break
                }
            }
            // @ts-ignore
            deviceItem.items.push({
                label: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                name: channelName,
                color: true,
                to: {
                    name: 'device-speed',
                    params: { deviceId: device.uid, name: channelName },
                },
                deviceUID: device.uid,
                duty: duty,
                rpm: rpm,
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
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lighting_modes.length === 0) {
                continue
            }
            // @ts-ignore
            deviceItem.items.push({
                label: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                name: channelName,
                icon: mdiLedOn,
                iconStyle: `color: ${deviceSettings.sensorsAndChannels.get(channelName)!.color};`,
                to: {
                    name: 'device-lighting',
                    params: { deviceId: device.uid, name: channelName },
                },
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
            })
        }
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lcd_modes.length === 0) {
                continue
            }
            // @ts-ignore
            deviceItem.items.push({
                label: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                name: channelName,
                icon: mdiTelevisionShimmer,
                iconStyle: `color: ${deviceSettings.sensorsAndChannels.get(channelName)!.color};`,
                to: { name: 'device-lcd', params: { deviceId: device.uid, name: channelName } },
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
            })
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
