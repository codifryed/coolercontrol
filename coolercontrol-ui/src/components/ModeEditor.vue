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
import { useSettingsStore } from '@/stores/SettingsStore'
import Button from 'primevue/button'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import { computed, inject, nextTick, onMounted, ref, type Ref, watch } from 'vue'
import InputText from 'primevue/inputtext'
import { useDeviceStore } from '@/stores/DeviceStore'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import { UID } from '@/models/Device.ts'
import { Mode } from '@/models/Mode.ts'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiChip } from '@mdi/js'
import { DeviceSettingReadDTO } from '@/models/DaemonSettings.ts'

interface Props {
    modeUID: UID
}

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const props: Props = dialogRef.value.data

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const currentMode: Ref<Mode> = computed(
    () => settingsStore.modes.find((mode) => mode.uid === props.modeUID)!,
)
const givenName: Ref<string> = ref(currentMode.value.name)
const inputArea = ref()
const deviceTableData: Ref<Array<DeviceData>> = ref([])

interface DeviceData {
    rowID: string
    deviceUID: string
    deviceName: string
    channelID: string
    channelColor: string
    channelLabel: string
    info: string
}

const initTableData = () => {
    deviceTableData.value.length = 0
    const modeSettings: Map<UID, Map<string, DeviceSettingReadDTO>> = new Map()
    for (const [deviceUID, settings] of currentMode.value.device_settings) {
        const channelSettings = new Map()
        for (const setting of settings) {
            channelSettings.set(setting.channel_name, setting)
        }
        modeSettings.set(deviceUID, channelSettings)
    }

    for (const device of deviceStore.allDevices()) {
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)
        if (deviceSettings == null || device.info == null) {
            continue
        }
        // Devices and Channels have been pre-sorted, unlike mode device settings.
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            const channelSettings = deviceSettings.sensorsAndChannels.get(channelName)
            if (channelSettings == null) {
                continue
            }
            const channelModeSetting = modeSettings.get(device.uid)?.get(channelName)
            let info: string = 'Unknown'
            if (channelInfo.speed_options != null) {
                if (channelModeSetting == null) {
                    // This means there doesn't exist a setting for this channel.
                    continue
                    // info = 'Default Profile'
                    // Displaying 'null' as a Default Profile is an issue if one mode has a
                    // setting for a channel and another mode doesn't. Then switching won't set
                    //  it back to 'default'. By not displaying the setting, as least we are
                    // indicating to the user that there is no setting for this channel.
                } else if (channelModeSetting.speed_fixed != null) {
                    info = `Manual: ${channelModeSetting.speed_fixed}%`
                } else if (channelModeSetting.profile_uid != null) {
                    info =
                        channelModeSetting.profile_uid === '0'
                            ? 'Default Profile'
                            : `Profile: ${
                                  settingsStore.profiles.find(
                                      (profile) => profile.uid === channelModeSetting.profile_uid,
                                  )?.name ?? 'Unknown'
                              }`
                }
            } else if (channelInfo.lighting_modes.length > 0) {
                if (channelModeSetting == null) {
                    continue
                    // info = 'Lighting Mode: None'
                } else {
                    info = `Lighting Mode: ${
                        channelModeSetting.lighting?.mode ?? 'Unknown'
                    } ; Colors: ${channelModeSetting.lighting?.colors.length ?? 'Unknown'}`
                }
            } else if (channelInfo.lcd_info != null) {
                if (channelModeSetting == null) {
                    continue
                    // info = 'LCD Mode: None'
                } else {
                    info = `LCD Mode: ${channelModeSetting.lcd?.mode ?? 'Unknown'}`
                }
            } else {
                // Then this channel is not controllable. i.e. Load or Freq.
                continue
            }
            deviceTableData.value.push({
                rowID: `${device.uid}-${channelName}`,
                deviceUID: device.uid,
                deviceName: device.name,
                channelID: channelName,
                channelColor: channelSettings.color,
                channelLabel: channelSettings.name,
                info: info,
            })
        }
    }
}
initTableData()

const saveMode = async () => {
    if (givenName.value === currentMode.value.name) {
        return // no change
    }
    const successful = await settingsStore.updateMode(currentMode.value.uid, givenName.value)
    if (successful) {
        currentMode.value.name = givenName.value
        dialogRef.value.close()
    }
}

const activateMode = async () => {
    const successful = await settingsStore.activateMode(currentMode.value.uid)
    if (successful) {
        dialogRef.value.close()
    }
}

nextTick(async () => {
    const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
    await delay()
    inputArea.value.$el.focus()
})

onMounted(async () => {
    watch(settingsStore.allUIDeviceSettings, () => {
        initTableData()
    })
})
</script>

<template>
    <div class="grid">
        <div class="col-fixed" style="width: 16rem">
            <span class="p-float-label mt-4">
                <InputText
                    ref="inputArea"
                    id="name"
                    v-model="givenName"
                    class="w-full"
                    @keydown.enter="saveMode"
                />
                <label for="name">Name</label>
            </span>
            <div class="mt-6">
                <Button
                    label="Save Name"
                    class="w-full"
                    :disabled="givenName === currentMode.name"
                    @click="saveMode"
                >
                    <span class="p-button-label">Save Name</span>
                </Button>
            </div>
            <div class="mt-5">
                <Button
                    label="Activate"
                    class="w-full"
                    @click="activateMode"
                    :disabled="currentMode.uid === settingsStore.modeActive"
                >
                    <span class="p-button-label">Activate</span>
                </Button>
            </div>
        </div>
        <div class="col pb-0 table-wrapper">
            <DataTable
                class="mt-3"
                :value="deviceTableData"
                row-group-mode="rowspan"
                :group-rows-by="['deviceName', 'rowID']"
            >
                <Column field="deviceName" header="Device">
                    <template #body="slotProps">
                        <div class="flex align-items-center">
                            <div class="flex-inline mr-2 pt-1">
                                <svg-icon
                                    type="mdi"
                                    :path="mdiChip"
                                    :size="deviceStore.getREMSize(1.3)"
                                />
                            </div>
                            <div>{{ slotProps.data.deviceName }}</div>
                        </div>
                    </template>
                </Column>
                <!-- This workaround with rowID is needed because of an issue with DataTable and rowGrouping -->
                <!-- Otherwise channelLabels from other devices are grouped together if they have the same name -->
                <Column field="rowID" header="Channel">
                    <template #body="slotProps">
                        <span
                            class="pi pi-minus mr-2"
                            :style="{ color: slotProps.data.channelColor }"
                        />{{ slotProps.data.channelLabel }}
                    </template>
                </Column>
                <Column field="setting" header="Setting">
                    <template #body="slotProps">
                        {{ slotProps.data.info }}
                    </template>
                </Column>
            </DataTable>
        </div>
    </div>
</template>

<style scoped lang="scss">
.table-wrapper :deep(.p-datatable-wrapper) {
    border-radius: 12px;
}
</style>
