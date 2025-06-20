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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiBookmarkCheckOutline, mdiInformationSlabCircleOutline, mdiMemory } from '@mdi/js'
import { useSettingsStore } from '@/stores/SettingsStore'
import { computed, onMounted, type Ref, ref, watch } from 'vue'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { Mode } from '@/models/Mode.ts'
import { UID } from '@/models/Device.ts'
import { DeviceSettingReadDTO } from '@/models/DaemonSettings.ts'
import Button from 'primevue/button'
import { useI18n } from 'vue-i18n'

interface Props {
    modeUID: UID
}

const props = defineProps<Props>()
const { t } = useI18n()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const currentMode: Ref<Mode> = computed(
    () => settingsStore.modes.find((mode) => mode.uid === props.modeUID)!,
)
const deviceTableData: Ref<Array<DeviceData>> = ref([])

interface DeviceData {
    rowID: string
    deviceUID: string
    deviceName: string
    channelID: string
    channelColor: string
    channelLabel: string
    settingType: string
    settingInfo: string
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
            let settingType = 'Unknown'
            let settingInfo: string = 'Unknown'
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
                    settingType = 'Manual'
                    settingInfo = `${channelModeSetting.speed_fixed}%`
                } else if (channelModeSetting.profile_uid != null) {
                    settingType = 'Profile'
                    settingInfo =
                        channelModeSetting.profile_uid === '0'
                            ? 'Default Profile'
                            : (settingsStore.profiles.find(
                                  (profile) => profile.uid === channelModeSetting.profile_uid,
                              )?.name ?? 'Unknown')
                }
            } else if (channelInfo.lighting_modes.length > 0) {
                if (channelModeSetting == null) {
                    continue
                    // info = 'Lighting Mode: None'
                } else {
                    settingType = 'Lighting Mode'
                    settingInfo = `${
                        channelModeSetting.lighting?.mode ?? 'Unknown'
                    } ; Colors: ${channelModeSetting.lighting?.colors.length ?? 'Unknown'}`
                }
            } else if (channelInfo.lcd_info != null) {
                if (channelModeSetting == null) {
                    continue
                    // info = 'LCD Mode: None'
                } else {
                    settingType = 'LCD Mode'
                    settingInfo = channelModeSetting.lcd?.mode ?? 'Unknown'
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
                settingType: settingType,
                settingInfo: settingInfo,
            })
        }
    }
}
initTableData()

const isActivated = false
const activateMode = async (): Promise<void> => {
    await settingsStore.activateMode(props.modeUID)
}

onMounted(async () => {
    watch(settingsStore.allUIDeviceSettings, () => {
        initTableData()
    })
})
</script>

<template>
    <div class="flex h-[3.6rem] border-b-4 border-border-one items-center justify-between">
        <div class="flex flex-row overflow-hidden">
            <div class="flex pl-4 py-2 text-2xl overflow-hidden">
                <span class="font-bold overflow-hidden overflow-ellipsis">{{
                    currentMode.name
                }}</span>
            </div>
            <div
                class="px-4 py-2 flex flex-row leading-none items-center"
                v-tooltip.top="t('views.mode.modeHint')"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiInformationSlabCircleOutline"
                    :size="deviceStore.getREMSize(1.25)"
                />
            </div>
        </div>
        <div
            class="p-2"
            v-tooltip.bottom="{ value: t('views.mode.currentlyActive'), disabled: !isActivated }"
        >
            <Button
                class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                label="Save"
                v-tooltip.bottom="t('views.mode.activateMode')"
                :disabled="isActivated"
                @click="activateMode"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiBookmarkCheckOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
        </div>
    </div>
    <div class="h-full pb-14">
        <DataTable
            :value="deviceTableData"
            row-group-mode="rowspan"
            :group-rows-by="['deviceName', 'rowID']"
            scrollable
            scroll-height="flex"
            :pt="{
                tableContainer: () => ({
                    class: ['rounded-none border-0 border-border-one'],
                }),
            }"
        >
            <Column field="deviceName" :header="t('components.sensorTable.device')">
                <template #body="slotProps">
                    <div class="flex leading-none items-center">
                        <div class="mr-2">
                            <svg-icon
                                type="mdi"
                                :path="mdiMemory"
                                :size="deviceStore.getREMSize(1.3)"
                            />
                        </div>
                        <div>{{ slotProps.data.deviceName }}</div>
                    </div>
                </template>
            </Column>
            <!-- This workaround with rowID is needed because of an issue with DataTable and rowGrouping -->
            <!-- Otherwise channelLabels from other devices are grouped together if they have the same name -->
            <Column field="rowID" :header="t('components.sensorTable.channel')">
                <template #body="slotProps">
                    <span
                        class="pi pi-minus mr-2"
                        :style="{ color: slotProps.data.channelColor }"
                    />{{ slotProps.data.channelLabel }}
                </template>
            </Column>
            <Column field="settingType" :header="t('components.modeTable.setting')">
                <template #body="slotProps">
                    {{ slotProps.data.settingType }}
                </template>
            </Column>
            <Column field="settingInfo" header="">
                <template #body="slotProps">
                    {{ slotProps.data.settingInfo }}
                </template>
            </Column>
        </DataTable>
    </div>
</template>

<style scoped lang="scss"></style>
