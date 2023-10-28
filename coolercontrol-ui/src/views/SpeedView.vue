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

import Dropdown from "primevue/dropdown"
import ToggleButton from 'primevue/togglebutton'
import {ref, type Ref} from "vue"
import {Profile, ProfileType} from "@/models/Profile"
import {useSettingsStore} from "@/stores/SettingsStore"
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {mdiContentSaveMoveOutline} from "@mdi/js";
import Button from "primevue/button";
import SpeedDefaultChart from "@/components/SpeedDefaultChart.vue";
import SpeedFixedChart from "@/components/SpeedFixedChart.vue";
import SpeedGraphChart from "@/components/SpeedGraphChart.vue";
import {type UID} from "@/models/Device";
import {useDeviceStore} from "@/stores/DeviceStore";
import MiniGauge from "@/components/MiniGauge.vue";
import Knob from "primevue/knob";
import {storeToRefs} from "pinia";
import {DeviceSettingReadDTO, DeviceSettingWriteManualDTO, DeviceSettingWriteProfileDTO} from "@/models/DaemonSettings";

interface Props {
  deviceId: UID
  name: string
}

const props = defineProps<Props>()

const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const {currentDeviceStatus} = storeToRefs(deviceStore)
let startingManualControlEnabled = false
let startingProfile = settingsStore.profiles.find((profile) => profile.uid === '0')! // default profile as default
const startingDeviceSetting: DeviceSettingReadDTO | undefined = settingsStore.allDaemonDeviceSettings
    .get(props.deviceId)
    ?.settings.get(props.name)
if (startingDeviceSetting?.speed_fixed != null) {
  startingManualControlEnabled = true
} else if (startingDeviceSetting?.profile_uid != null) {
  startingProfile = settingsStore.profiles.find((profile) => profile.uid === startingDeviceSetting!.profile_uid)!
}
const selectedProfile: Ref<Profile> = ref(startingProfile);
const manualControlEnabled: Ref<boolean> = ref(startingManualControlEnabled)
const getCurrentDuty = (): number | undefined => {
  const duty = currentDeviceStatus.value.get(props.deviceId)?.get(props.name)?.duty
  return duty != null ? Number(duty) : undefined
}

const manualDuty = ref(getCurrentDuty())
const dutyMin = 0
const dutyMax = 100

const channelIsControllable = (): boolean => {
  for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceId && device.info != null) {
      const channelInfo = device.info.channels.get(props.name)
      if (channelInfo != null && channelInfo.speed_options != null) {
        return true
      }
    }
  }
  return false
}

const getProfileOptions = (): any[] => {
  if (channelIsControllable()) {
    return settingsStore.profiles
  } else {
    return [settingsStore.profiles.find(profile => profile.uid === '0')]
  }
}

const saveProfileSetting = async () => {
  const setting = new DeviceSettingWriteProfileDTO(selectedProfile.value.uid)
  await settingsStore.saveDaemonDeviceSettingProfile(props.deviceId, props.name, setting)
}

const onManualChangeFinished = async (_: Event): Promise<void> => {
  const sleep = (ms: number) => new Promise(r => setTimeout(r, ms))
  await sleep(10) // workaround: a simple click on the knob takes a moment before the manualDuty is updated
  if (manualDuty.value == null) {
    return
  }
  const setting = new DeviceSettingWriteManualDTO(manualDuty.value);
  await settingsStore.saveDaemonDeviceSettingManual(props.deviceId, props.name, setting)
}

</script>

<template>
  <div class="card pt-2">
    <div class="grid">
      <div class="col-fixed" style="width: 13.75rem">
        <div v-if="channelIsControllable()" class="mt-2">
          <ToggleButton v-model="manualControlEnabled" class="w-full" on-label="Manual" off-label="Profiles"/>
        </div>
        <div class="p-float-label mt-5">
          <Dropdown v-model="selectedProfile" inputId="dd-profile" :options="getProfileOptions()" option-label="name"
                    placeholder="Profile" class="w-full" scroll-height="flex" :disabled="manualControlEnabled"/>
          <label for="dd-profile">Profile</label>
        </div>
        <Button label="Apply" size="small" rounded class="mt-5"
                :disabled="manualControlEnabled" @click="saveProfileSetting">
          <svg-icon class="p-button-icon p-button-icon-left pi" type="mdi" :path="mdiContentSaveMoveOutline"
                    size="1.35rem"/>
          <span class="p-button-label">Apply</span>
        </Button>
        <div v-if="!manualControlEnabled">
          <div v-if="selectedProfile.p_type === ProfileType.Graph" class="mt-6">
            <MiniGauge :device-u-i-d="selectedProfile.temp_source!.device_uid"
                       :sensor-name="selectedProfile.temp_source!.temp_name"
                       :key="'temp'+props.deviceId+props.name+selectedProfile.uid"/>
            <MiniGauge :device-u-i-d="props.deviceId"
                       :sensor-name="props.name" :key="'duty'+props.deviceId+props.name+selectedProfile.uid"/>
          </div>
        </div>
      </div>
      <div class="col pb-0">
        <div v-if="manualControlEnabled">
          <Knob v-model="manualDuty" valueTemplate="{value}%" :min="dutyMin" :max="dutyMax" :step="1"
                :size="deviceStore.getREMSize(20)"
                class="text-center mt-3" @mouseup="onManualChangeFinished"/>
        </div>
        <div v-else>
          <SpeedDefaultChart v-if="selectedProfile.p_type === ProfileType.Default"
                             :profile="selectedProfile" :current-device-u-i-d="props.deviceId"
                             :current-sensor-name="props.name"
                             :key="'default'+props.deviceId+props.name+selectedProfile.uid"/>
          <SpeedFixedChart v-else-if="selectedProfile.p_type === ProfileType.Fixed"
                           :profile="selectedProfile" :current-device-u-i-d="props.deviceId"
                           :current-sensor-name="props.name"
                           :key="'fixed'+props.deviceId+props.name+selectedProfile.uid"/>
          <SpeedGraphChart v-else-if="selectedProfile.p_type === ProfileType.Graph"
                           :profile="selectedProfile" :current-device-u-i-d="props.deviceId"
                           :current-sensor-name="props.name"
                           :key="'graph'+props.deviceId+props.name+selectedProfile.uid"/>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">

</style>