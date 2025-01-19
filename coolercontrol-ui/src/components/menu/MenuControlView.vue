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
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { computed } from 'vue'
import { DeviceSettingReadDTO } from '@/models/DaemonSettings.ts'

interface Props {
    deviceUID: UID
    channelName: string
}

const props = defineProps<Props>()
const settingsStore = useSettingsStore()
const controlSetting = computed(() => {
    const deviceSetting: DeviceSettingReadDTO | undefined = settingsStore.allDaemonDeviceSettings
        .get(props.deviceUID)
        ?.settings.get(props.channelName)
    if (deviceSetting?.speed_fixed != null) {
        return `Manual: ${deviceSetting!.speed_fixed}%`
    } else if (deviceSetting?.profile_uid != null) {
        const profileName =
            settingsStore.profiles.find((profile) => profile.uid === deviceSetting!.profile_uid)
                ?.name ?? 'Unknown'
        return `Profile: ${profileName}`
    } else {
        // Nothing has been set, so settings are blank
        return 'Profile: Default Profile'
    }
})
</script>
<template>
    <div class="flex leading-none">
        {{ controlSetting }}
    </div>
</template>

<style scoped lang="scss"></style>
