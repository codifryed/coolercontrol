<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2025  Guy Boldon and contributors
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
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { UID } from '@/models/Device.ts'
import { computed } from 'vue'

interface Props {
    deviceUID: UID
    channelName: string
}

const props = defineProps<Props>()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()

const modeName = computed(() => {
    const deviceSetting = settingsStore.allDaemonDeviceSettings
        .get(props.deviceUID)
        ?.settings.get(props.channelName)
    const settingModeName = deviceSetting?.lighting?.mode ?? deviceSetting?.lcd?.mode
    if (settingModeName == null) {
        return 'None'
    }
    for (const device of deviceStore.allDevices()) {
        if (device.uid !== props.deviceUID) continue
        const channelInfo = device.info?.channels.get(props.channelName)
        if (channelInfo == null) break
        const mode =
            channelInfo.lighting_modes.find((m) => m.name === settingModeName) ??
            channelInfo.lcd_modes.find((m) => m.name === settingModeName)
        if (mode != null) {
            return mode.frontend_name
        }
        break
    }
    return 'None'
})
</script>
<template>
    <div class="flex leading-tight tree-text">
        {{ modeName }}
    </div>
</template>

<style scoped lang="scss"></style>
