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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiFan, mdiLedOn, mdiMemory, mdiTelevisionShimmer } from '@mdi/js'
import { ChannelValues, useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import Fieldset from 'primevue/fieldset'
import Divider from 'primevue/divider'
import { onMounted, Ref, ref, watch } from 'vue'
import { Dashboard } from '@/models/Dashboard.ts'
import { DeviceType, UID } from '@/models/Device.ts'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

interface Props {
    dashboard: Dashboard
}

const props = defineProps<Props>()
const includesDevice = (deviceUID: UID): boolean =>
    props.dashboard.deviceChannelNames.length === 0 ||
    props.dashboard.deviceChannelNames.some(
        (deviceChannel) => deviceChannel.deviceUID === deviceUID,
    )
const includesDeviceChannel = (deviceUID: UID, channelName: string): boolean =>
    props.dashboard.deviceChannelNames.length === 0 ||
    props.dashboard.deviceChannelNames.some(
        (deviceChannel) =>
            deviceChannel.deviceUID === deviceUID && deviceChannel.channelName === channelName,
    )

const deviceControlData: Ref<Array<DeviceControlData>> = ref([])

interface DeviceControlData {
    deviceUID: string
    deviceLabel: string
    icon: any
    channelData: Array<ChannelControlData>
}

interface ChannelControlData {
    channelID: string
    channelLabel: string
    channelColor: string
    icon: any
    to: any // route for Control Page
    duty?: number
    rpm?: number
    profileLabel?: string
    profileTo?: any
    lcdMode?: string
    lightingMode?: string
}

const initWidgetData = () => {
    deviceControlData.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (!includesDevice(device.uid)) continue
        if (device.type === DeviceType.CUSTOM_SENSORS || device.type === DeviceType.CPU) {
            continue // no controls
        }
        if (device.info == null) {
            continue
        }
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        const deviceDaemonSettings = settingsStore.allDaemonDeviceSettings.get(device.uid)
        const deviceData: DeviceControlData = {
            deviceUID: device.uid,
            deviceLabel: deviceSettings.name,
            icon: mdiMemory,
            channelData: [],
        }
        // speed channels first
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.speed_options == null) continue
            if (!includesDeviceChannel(device.uid, channelName)) continue
            const isControllable: boolean = channelInfo.speed_options.fixed_enabled ?? false
            if (!isControllable) continue
            let duty: number | undefined = undefined
            let rpm: number | undefined = undefined
            for (const channel of device.status.channels) {
                if (channel.name === channelName) {
                    duty = channel.duty
                    rpm = channel.rpm
                    break
                }
            }
            // Profile data
            // Set default to when no settings have been set/blank, so are the default:
            let profileUID: string | undefined = '0'
            let profileLabel: string | undefined = 'Default Profile'
            let profileTo: any | undefined = undefined
            if (deviceDaemonSettings?.settings.get(channelName) != null) {
                if (deviceDaemonSettings?.settings.get(channelName)!.speed_fixed != null) {
                    profileUID = undefined
                    profileLabel = 'Manual'
                    profileTo = undefined
                } else if (deviceDaemonSettings?.settings.get(channelName)!.profile_uid != null) {
                    profileUID = deviceDaemonSettings?.settings.get(channelName)!.profile_uid
                    const profile = settingsStore.profiles.find(
                        (profile) => profile.uid === profileUID,
                    )!
                    profileLabel = profile.name
                    profileTo =
                        profileUID === '0'
                            ? undefined
                            : {
                                  name: 'profiles',
                                  params: { profileUID: profileUID },
                              }
                }
            }
            deviceData.channelData.push({
                channelID: channelName,
                channelLabel: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                channelColor: deviceSettings.sensorsAndChannels.get(channelName)!.color,
                icon: mdiFan,
                to: {
                    name: 'device-speed',
                    params: { deviceUID: device.uid, channelName: channelName },
                },
                duty: duty,
                rpm: rpm,
                profileLabel: profileLabel,
                profileTo: profileTo,
            })
        }
        // lighting channels
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lighting_modes.length === 0) continue
            if (!includesDeviceChannel(device.uid, channelName)) continue
            let lightingMode = 'None'
            if (deviceDaemonSettings?.settings.get(channelName) != null) {
                const modeName = deviceDaemonSettings?.settings.get(channelName)!.lighting?.mode
                const modeFrontendName = channelInfo.lighting_modes.find(
                    (mode) => mode.name === modeName,
                )?.frontend_name
                if (modeFrontendName != null) {
                    lightingMode = modeFrontendName
                }
            }
            deviceData.channelData.push({
                channelID: channelName,
                channelLabel: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                channelColor: deviceSettings.sensorsAndChannels.get(channelName)!.color,
                icon: mdiLedOn,
                to: {
                    name: 'device-lighting',
                    params: { deviceId: device.uid, channelName: channelName },
                },
                lightingMode: lightingMode,
            })
        }
        // lcd channels
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lcd_modes.length === 0) continue
            if (!includesDeviceChannel(device.uid, channelName)) continue
            let lcdMode = 'None'
            if (deviceDaemonSettings?.settings.get(channelName) != null) {
                const modeName = deviceDaemonSettings?.settings.get(channelName)!.lcd?.mode
                const modeFrontendName = channelInfo.lcd_modes.find(
                    (mode) => mode.name === modeName,
                )?.frontend_name
                if (modeFrontendName != null) {
                    lcdMode = modeFrontendName
                }
            }
            deviceData.channelData.push({
                channelID: channelName,
                channelLabel: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                channelColor: deviceSettings.sensorsAndChannels.get(channelName)!.color,
                icon: mdiTelevisionShimmer,
                to: {
                    name: 'device-lcd',
                    params: { deviceId: device.uid, channelName: channelName },
                },
                lcdMode: lcdMode,
            })
        }
        if (deviceData.channelData.length === 0) continue
        deviceControlData.value.push(deviceData)
    }
}

initWidgetData()

const deviceChannelValues = (deviceUID: UID, channelName: string): ChannelValues | undefined =>
    deviceStore.currentDeviceStatus.get(deviceUID)?.get(channelName)

//----------------------------------------------------------------------------------------------------------------------

onMounted(async () => {
    watch(settingsStore.allUIDeviceSettings, () => {
        initWidgetData()
    })
})
</script>

<template>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="pb-24 h-screen w-full">
            <div v-for="deviceControl in deviceControlData" :key="deviceControl.deviceUID">
                <Divider align="left" class="!my-0 !mt-5">
                    <div class="flex flex-row text-xl">
                        <svg-icon
                            v-if="deviceControl.icon"
                            class="mr-1.5 min-w-6 text-text-color"
                            type="mdi"
                            :path="deviceControl.icon ?? ''"
                            :size="deviceStore.getREMSize(1.75)"
                        />
                        {{ deviceControl.deviceLabel }}
                    </div>
                </Divider>
                <div class="flex flex-row flex-wrap mb-16">
                    <div
                        v-for="channelControl in deviceControl.channelData"
                        :key="channelControl.channelID"
                        class="pl-4 mt-5"
                    >
                        <router-link :to="channelControl.to ?? ''">
                            <Fieldset class="w-80 h-[8.5rem] bg-bg-two/85 hover:bg-bg-two/100">
                                <template #legend>
                                    <div class="text-ellipsis flex flex-row items-center">
                                        <svg-icon
                                            v-if="channelControl.icon"
                                            class="mr-1.5 min-w-6"
                                            type="mdi"
                                            :path="channelControl.icon ?? ''"
                                            :style="{ color: channelControl.channelColor }"
                                            :size="deviceStore.getREMSize(1.75)"
                                        />
                                        {{ channelControl.channelLabel }}
                                    </div>
                                </template>
                                <table class="w-full">
                                    <tbody>
                                        <tr>
                                            <td class="text-left w-full text-ellipsis">
                                                <div v-if="channelControl.profileTo != null">
                                                    <router-link
                                                        :to="channelControl.profileTo"
                                                        class="underline hover:text-accent"
                                                    >
                                                        {{ channelControl.profileLabel }}
                                                    </router-link>
                                                </div>
                                                <div v-else>
                                                    {{
                                                        channelControl.profileLabel ??
                                                        channelControl.lightingMode ??
                                                        channelControl.lcdMode ??
                                                        'Unknown'
                                                    }}
                                                </div>
                                            </td>
                                            <td class="text-right">
                                                <div v-if="channelControl.duty != null">
                                                    {{
                                                        deviceChannelValues(
                                                            deviceControl.deviceUID,
                                                            channelControl.channelID,
                                                        )?.duty
                                                    }}
                                                    <span style="font-size: 0.7rem"
                                                        >%&nbsp;&nbsp;&nbsp;</span
                                                    >
                                                </div>
                                            </td>
                                        </tr>
                                        <tr>
                                            <td class="text-left text-ellipsis"></td>
                                            <td class="text-right">
                                                <div v-if="channelControl.rpm != null">
                                                    {{
                                                        deviceChannelValues(
                                                            deviceControl.deviceUID,
                                                            channelControl.channelID,
                                                        )?.rpm
                                                    }}
                                                    <span style="font-size: 0.7rem">rpm</span>
                                                </div>
                                            </td>
                                        </tr>
                                    </tbody>
                                </table>
                            </Fieldset>
                        </router-link>
                    </div>
                </div>
            </div>
        </ScrollAreaViewport>
        <ScrollAreaScrollbar
            class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
            orientation="vertical"
        >
            <ScrollAreaThumb
                class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
            />
        </ScrollAreaScrollbar>
    </ScrollAreaRoot>
</template>

<style lang="scss" scoped></style>
