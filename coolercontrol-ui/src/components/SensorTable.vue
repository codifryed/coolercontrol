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
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import { Ref, onMounted, ref, watch } from 'vue'
import { Status } from '@/models/Status'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiChip } from '@mdi/js'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const deviceTableData: Ref<Array<DeviceData>> = ref([])

enum ChannelType {
    duty = 'duty',
    rpm = 'rpm',
    temp = 'temp',
    freq = 'freq',
}

interface DeviceData {
    rowID: string
    deviceUID: string
    deviceName: string
    channelID: string
    channelColor: string
    channelLabel: string
    channelType: ChannelType
    value: number
    min: number
    max: number
}

const calcMaxMinValues = (
    channel_name: string,
    status_history: Array<Status>,
    channelType: ChannelType,
): [number, number] => {
    const channelValues: Array<number> = []
    status_history
        .map((status) =>
            channelType == ChannelType.duty
                ? status.channels.find((channel) => channel.name === channel_name)?.duty ?? 0
                : channelType == ChannelType.rpm
                  ? status.channels.find((channel) => channel.name === channel_name)?.rpm ?? 0
                  : channelType == ChannelType.freq
                    ? status.channels.find((channel) => channel.name === channel_name)?.freq ?? 0
                    : channelType == ChannelType.temp
                      ? status.temps.find((temp) => temp.name === channel_name)?.temp ?? 0
                      : 0,
        )
        .forEach((value) => channelValues.push(value))

    const min = channelValues.reduce(
        (accumulator, currentValue) => Math.min(accumulator, currentValue),
        Number.MAX_SAFE_INTEGER,
    )
    const max = channelValues.reduce(
        (accumulator, currentValue) => Math.max(accumulator, currentValue),
        0,
    )
    return [min, max]
}

const initTableData = () => {
    deviceTableData.value.length = 0
    for (const device of deviceStore.allDevices()) {
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        for (const temp of device.status.temps) {
            const channelSettings = deviceSettings.sensorsAndChannels.get(temp.name)
            if (channelSettings?.hide) {
                continue
            }
            const [min, max] = calcMaxMinValues(temp.name, device.status_history, ChannelType.temp)
            deviceTableData.value.push({
                rowID: device.uid + temp.name,
                deviceUID: device.uid,
                deviceName: deviceSettings.name,
                channelID: temp.name,
                channelColor: channelSettings?.color ?? 'white',
                channelType: ChannelType.temp,
                channelLabel: channelSettings?.name ?? temp.name,
                value: temp.temp,
                min: min,
                max: max,
            })
        }
        if (device.info == null) {
            continue
        }
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (channelInfo.lcd_info != null || channelInfo.lighting_modes.length > 0) {
                continue
            }
            const channelSettings = deviceSettings.sensorsAndChannels.get(channelName)
            if (channelSettings?.hide) {
                continue
            }
            for (const channel of device.status.channels) {
                if (channel.name !== channelName) {
                    continue
                }
                if (channel.duty != null) {
                    // handles both duty and load
                    const [min, max] = calcMaxMinValues(
                        channel.name,
                        device.status_history,
                        ChannelType.duty,
                    )
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        channelType: ChannelType.duty,
                        value: channel.duty,
                        min: min,
                        max: max,
                    })
                }
                if (channel.rpm != null) {
                    const [min, max] = calcMaxMinValues(
                        channel.name,
                        device.status_history,
                        ChannelType.rpm,
                    )
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        channelType: ChannelType.rpm,
                        value: channel.rpm,
                        min: min,
                        max: max,
                    })
                }
                if (channel.freq != null) {
                    const [min, max] = calcMaxMinValues(
                        channel.name,
                        device.status_history,
                        ChannelType.freq,
                    )
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        channelType: ChannelType.freq,
                        value: channel.freq,
                        min: min,
                        max: max,
                    })
                }
            }
        }
    }
}

initTableData()

const updateTableData = () => {
    for (const row of deviceTableData.value) {
        let newValue: number
        switch (row.channelType) {
            case ChannelType.temp:
                newValue = Number(
                    deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!.temp,
                )
                break
            case ChannelType.duty:
                newValue = Number(
                    deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!.duty,
                )
                break
            case ChannelType.rpm:
                newValue = Number(
                    deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!.rpm,
                )
                break
            case ChannelType.freq:
                newValue = Number(
                    deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!.freq,
                )
                break
        }
        row.value = newValue
        row.min = Math.min(row.min, newValue)
        row.max = Math.max(row.max, newValue)
    }
}

const format = (value: number, channelType: ChannelType): string => {
    switch (channelType) {
        case ChannelType.temp:
            return value.toFixed(1) + ' °'
        case ChannelType.duty:
            return value.toFixed(0) + ' %'
        case ChannelType.rpm:
            return value.toFixed(0) + ' rpm'
        case ChannelType.freq:
            return value.toFixed(0) + ' mhz'
    }
}

//----------------------------------------------------------------------------------------------------------------------

onMounted(async () => {
    deviceStore.$onAction(({ name, after }) => {
        if (name === 'updateStatus') {
            after((onlyRecentStatus: boolean) => {
                if (onlyRecentStatus) {
                    updateTableData()
                } else {
                    initTableData()
                }
            })
        }
    })

    watch(settingsStore.allUIDeviceSettings, () => {
        initTableData()
    })
})
</script>

<template>
    <div class="table-wrapper">
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
            <Column field="value" header="Value">
                <template #body="slotProps">
                    {{ format(slotProps.data.value, slotProps.data.channelType) }}
                </template>
            </Column>
            <Column field="min" header="Min">
                <template #body="slotProps">
                    {{ format(slotProps.data.min, slotProps.data.channelType) }}
                </template>
            </Column>
            <Column field="max" header="Max">
                <template #body="slotProps">
                    {{ format(slotProps.data.max, slotProps.data.channelType) }}
                </template>
            </Column>
        </DataTable>
    </div>
</template>

<style scoped>
.table-wrapper :deep(.p-datatable-wrapper) {
    border-radius: 12px;
}
</style>
