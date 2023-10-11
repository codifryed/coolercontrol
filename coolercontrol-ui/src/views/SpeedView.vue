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
import {onMounted, ref, type Ref, watch} from "vue"
import {Profile, ProfileType} from "@/models/Profile"
import {useSettingsStore} from "@/stores/SettingsStore"
import SvgIcon from '@jamescoyle/vue-icon'
import {mdiContentSaveMoveOutline} from "@mdi/js";
import Button from "primevue/button";
import SpeedDefaultChart from "@/components/SpeedDefaultChart.vue";
import SpeedFixedChart from "@/components/SpeedFixedChart.vue";
import SpeedGraphChart from "@/components/SpeedGraphChart.vue";
import {type UID} from "@/models/Device";
import {useDeviceStore} from "@/stores/DeviceStore";

interface Props {
  deviceId: UID
  name: string
}

const props = defineProps<Props>()

const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
// todo: load from "settings" the saved selectedProfile, if none, then the default:
const selectedProfile: Ref<Profile> = ref(settingsStore.profiles.find((profile) => profile.orderId === 0)!)
const settingsChanged = ref(false)

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

const getProfileOptions = () => {
  if (channelIsControllable()) {
    return settingsStore.profiles
  } else {
    return [Profile.createDefault()]
  }
}

const saveSpeedConfig = () => {

  settingsChanged.value = false
}

onMounted(() => {
  watch(selectedProfile, () => {
    settingsChanged.value = true
  })
})
</script>

<template>
  <div class="card pt-6">
    <div class="grid">
      <div class="col-fixed" style="width: 220px">
        <div class="p-float-label mt-4">
          <Dropdown v-model="selectedProfile" inputId="dd-profile" :options="getProfileOptions()" option-label="name"
                    placeholder="Profile" class="w-full"/>
          <label for="dd-profile">Profile</label>
          <!--          todo: function dropdown-->
          <Button label="Apply" size="small" rounded class="mt-4"
                  :disabled="!settingsChanged" @click="saveSpeedConfig">
            <svg-icon class="p-button-icon p-button-icon-left pi" type="mdi" :path="mdiContentSaveMoveOutline"
                      size="1.35rem"/>
            <span class="p-button-label">Apply</span>
          </Button>
        </div>
      </div>
      <div class="col">
        <SpeedDefaultChart v-if="selectedProfile.type === ProfileType.DEFAULT"
                           :profile="selectedProfile" :current-device-u-i-d="props.deviceId"
                           :current-sensor-name="props.name" :key="props.deviceId+props.name+'default'"/>
        <SpeedFixedChart v-else-if="selectedProfile.type === ProfileType.FIXED"
                         :profile="selectedProfile" :current-device-u-i-d="props.deviceId"
                         :current-sensor-name="props.name" :key="props.deviceId+props.name+'fixed'"/>
        <SpeedGraphChart v-else-if="selectedProfile.type === ProfileType.GRAPH"
                         :profile="selectedProfile" :current-device-u-i-d="props.deviceId"
                         :current-sensor-name="props.name" :key="props.deviceId+props.name+'graph'"/>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">

</style>