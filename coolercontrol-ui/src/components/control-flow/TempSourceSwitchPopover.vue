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
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Popover from 'primevue/popover'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useDeviceStore } from '@/stores/DeviceStore'
import { ProfileTempSource } from '@/models/Profile'
import type { UID } from '@/models/Device'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiCheck } from '@mdi/js'

const props = defineProps<{
    profileUID: UID
    currentDeviceUID?: string
    currentTempName?: string
}>()

const emit = defineEmits<{
    (e: 'changed'): void
}>()

const { t } = useI18n()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const popRef = ref()

interface TempOption {
    deviceUID: string
    tempName: string
    tempLabel: string
    tempColor: string
    temp?: string
}

interface DeviceGroup {
    deviceUID: string
    deviceName: string
    temps: TempOption[]
}

const deviceGroups = computed<DeviceGroup[]>(() => {
    const groups: DeviceGroup[] = []
    for (const device of deviceStore.allDevices()) {
        if (device.status.temps.length === 0 || device.info == null) continue
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)
        if (!deviceSettings) continue
        const temps: TempOption[] = []
        for (const temp of device.status.temps) {
            temps.push({
                deviceUID: device.uid,
                tempName: temp.name,
                tempLabel: deviceSettings.sensorsAndChannels.get(temp.name)?.name ?? temp.name,
                tempColor: deviceSettings.sensorsAndChannels.get(temp.name)?.color ?? '#568af2',
                temp: temp.temp.toFixed(1),
            })
        }
        if (temps.length > 0) {
            groups.push({
                deviceUID: device.uid,
                deviceName: deviceSettings.name ?? device.uid,
                temps,
            })
        }
    }
    return groups
})

function toggle(event: Event) {
    popRef.value?.toggle(event)
}

function isCurrent(deviceUID: string, tempName: string): boolean {
    return deviceUID === props.currentDeviceUID && tempName === props.currentTempName
}

async function selectTemp(deviceUID: string, tempName: string) {
    if (isCurrent(deviceUID, tempName)) {
        popRef.value?.hide()
        return
    }
    const profile = settingsStore.profiles.find((p) => p.uid === props.profileUID)
    if (!profile) return
    profile.temp_source = new ProfileTempSource(tempName, deviceUID)
    await settingsStore.updateProfile(props.profileUID)
    popRef.value?.hide()
    emit('changed')
}

defineExpose({ toggle })
</script>

<template>
    <Popover ref="popRef" append-to="body">
        <div class="w-72 rounded-lg border border-border-one bg-bg-two p-3">
            <div class="mb-2 text-sm font-semibold text-text-color">
                {{ t('views.controls.switchTempSource') }}
            </div>
            <div class="max-h-72 overflow-y-auto">
                <template v-for="group in deviceGroups" :key="group.deviceUID">
                    <div
                        class="sticky top-0 bg-bg-two px-2 py-1 text-xs font-semibold text-text-color-secondary"
                    >
                        {{ group.deviceName }}
                    </div>
                    <div
                        v-for="temp in group.temps"
                        :key="`${temp.deviceUID}/${temp.tempName}`"
                        class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 transition-colors hover:bg-surface-hover"
                        :class="isCurrent(temp.deviceUID, temp.tempName) ? 'bg-accent/10' : ''"
                        @click="selectTemp(temp.deviceUID, temp.tempName)"
                    >
                        <svg-icon
                            v-if="isCurrent(temp.deviceUID, temp.tempName)"
                            type="mdi"
                            :path="mdiCheck"
                            class="size-4 shrink-0 text-accent"
                        />
                        <div v-else class="size-4 shrink-0" />
                        <span
                            class="size-2.5 shrink-0 rounded-full"
                            :style="{ backgroundColor: temp.tempColor }"
                        />
                        <span class="flex-1 truncate text-sm text-text-color">
                            {{ temp.tempLabel }}
                        </span>
                        <span v-if="temp.temp" class="text-xs text-text-color-secondary">
                            {{ temp.temp }}{{ t('common.tempUnit') }}
                        </span>
                    </div>
                </template>
            </div>
        </div>
    </Popover>
</template>
