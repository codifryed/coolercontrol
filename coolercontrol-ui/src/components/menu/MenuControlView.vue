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
import { UID } from '@/models/Device.ts'
import { computed } from 'vue'
import { DeviceSettingReadDTO } from '@/models/DaemonSettings.ts'
import { useI18n } from 'vue-i18n'

interface Props {
    deviceUID: UID
    channelName: string
    isControllable?: boolean
    isActive?: boolean
}

const props = withDefaults(defineProps<Props>(), {
    isControllable: true,
    isActive: false,
})
const settingsStore = useSettingsStore()
const { t } = useI18n()

const display = computed<{ text: string; isPlaceholder: boolean }>(() => {
    if (!props.isControllable) {
        return { text: t('common.readOnly'), isPlaceholder: true }
    }
    const deviceSetting: DeviceSettingReadDTO | undefined = settingsStore.allDaemonDeviceSettings
        .get(props.deviceUID)
        ?.settings.get(props.channelName)
    if (deviceSetting?.speed_fixed != null) {
        return { text: `${deviceSetting.speed_fixed}%`, isPlaceholder: false }
    }
    if (deviceSetting?.profile_uid != null && deviceSetting.profile_uid !== '0') {
        const name =
            settingsStore.profiles.find((profile) => profile.uid === deviceSetting.profile_uid)
                ?.name ?? 'Unknown'
        return { text: name, isPlaceholder: false }
    }
    return { text: t('common.unmanaged'), isPlaceholder: true }
})
</script>
<template>
    <div
        class="flex leading-tight tree-text"
        :class="{ 'italic text-text-color-secondary': display.isPlaceholder }"
    >
        {{ display.text }}
    </div>
</template>

<style scoped lang="scss"></style>
