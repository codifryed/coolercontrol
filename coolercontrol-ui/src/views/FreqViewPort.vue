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
import type { UID } from '@/models/Device.ts'
import { ref, type Ref, watch } from 'vue'
import _ from 'lodash'
import { v4 as uuidV4 } from 'uuid'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import FreqView from '@/views/FreqView.vue'

interface Props {
    deviceId: UID
    name: string
}

const props = defineProps<Props>()
const settingsStore = useSettingsStore()
const chartKey: Ref<string> = ref(uuidV4())
watch(
    [settingsStore.systemOverviewOptions, settingsStore.allUIDeviceSettings],
    _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
)
</script>

<template>
    <FreqView :device-id="props.deviceId" :name="props.name" :key="chartKey" />
</template>

<style scoped lang="scss"></style>
