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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiMemory } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import { onMounted, Ref, ref, watch } from 'vue'
import { Status } from '@/models/Status'
import { Dashboard, DataType } from '@/models/Dashboard.ts'
import { UID } from '@/models/Device.ts'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { useI18n } from 'vue-i18n'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()

interface Props {
    dashboard: Dashboard
}

const props = defineProps<Props>()
const includesTemps: boolean =
    props.dashboard.dataTypes.length === 0 || props.dashboard.dataTypes.includes(DataType.TEMP)
const includedDuties: boolean =
    props.dashboard.dataTypes.length === 0 || props.dashboard.dataTypes.includes(DataType.DUTY)
const includesLoads: boolean =
    props.dashboard.dataTypes.length === 0 || props.dashboard.dataTypes.includes(DataType.LOAD)
const includesFreqs: boolean =
    props.dashboard.dataTypes.length === 0 || props.dashboard.dataTypes.includes(DataType.FREQ)
const includesRPMs: boolean =
    props.dashboard.dataTypes.length === 0 || props.dashboard.dataTypes.includes(DataType.RPM)
const includesWatts: boolean =
    props.dashboard.dataTypes.length === 0 || props.dashboard.dataTypes.includes(DataType.WATTS)
const includesDevice = (deviceUID: UID): boolean =>
    props.dashboard.deviceChannelNames.length === 0 ||
    props.dashboard.deviceChannelNames.some(
        (deviceChannel) => deviceChannel.deviceUID === deviceUID,
    )
const includesDeviceChannel = (deviceUID: UID, channelName: string): boolean =>
    props.dashboard.deviceChannelNames.length === 0 ||
    props.dashboard.deviceChannelNames.some(
        (deviceChannel) =>
            deviceChannel.deviceUID === deviceUID && deviceChannel.channelName === channelName,
    )

const deviceTableData: Ref<Array<DeviceData>> = ref([])

interface DeviceData {
    rowID: string
    deviceUID: string
    deviceName: string
    channelID: string
    channelColor: string
    channelLabel: string
    dataType: DataType // we include LOAD in with DUTY since they are both percents.
    value: number
    min: number
    max: number
    avg: number
    count: number // number of values we're calculating
}

const getData = (status: Status, channel_name: string, dataType: DataType): number => {
    switch (dataType) {
        case DataType.DUTY:
            return status.channels.find((channel) => channel.name === channel_name)?.duty ?? 0
        case DataType.RPM:
            return status.channels.find((channel) => channel.name === channel_name)?.rpm ?? 0
        case DataType.FREQ:
            return status.channels.find((channel) => channel.name === channel_name)?.freq ?? 0
        case DataType.WATTS:
            return status.channels.find((channel) => channel.name === channel_name)?.watts ?? 0
        case DataType.TEMP:
            return status.temps.find((temp) => temp.name === channel_name)?.temp ?? 0
        default:
            return 0
    }
}

// We collect and calculate them all together to keep init processing time down
const calcMinMaxAvg = (
    channel_name: string,
    status_history: Array<Status>,
    dataType: DataType,
): [number, number, number, number] => {
    const channelValues: Array<number> = status_history.map((status) =>
        getData(status, channel_name, dataType),
    )
    const min = channelValues.reduce(
        (accumulator, currentValue) => Math.min(accumulator, currentValue),
        Number.MAX_SAFE_INTEGER,
    )
    const max = channelValues.reduce(
        (accumulator, currentValue) => Math.max(accumulator, currentValue),
        0,
    )
    const avg =
        channelValues.reduce((accumulator, currentValue) => accumulator + currentValue, 0) /
        channelValues.length
    const count = channelValues.length
    return [min, max, avg, count]
}

const initTableData = () => {
    deviceTableData.value.length = 0
    for (const device of deviceStore.allDevices()) {
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        if (!includesDevice(device.uid)) continue
        for (const temp of device.status.temps) {
            const channelSettings = deviceSettings.sensorsAndChannels.get(temp.name)
            if (!includesDeviceChannel(device.uid, temp.name) || !includesTemps) {
                continue
            }
            const [min, max, avg, count] = calcMinMaxAvg(
                temp.name,
                device.status_history,
                DataType.TEMP,
            )
            deviceTableData.value.push({
                rowID: device.uid + temp.name,
                deviceUID: device.uid,
                deviceName: deviceSettings.name,
                channelID: temp.name,
                channelColor: channelSettings?.color ?? 'white',
                dataType: DataType.TEMP,
                channelLabel: channelSettings?.name ?? temp.name,
                value: temp.temp,
                min: min,
                max: max,
                avg: avg,
                count: count,
            })
        }
        if (device.info == null) {
            continue
        }
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (
                !includesDeviceChannel(device.uid, channelName) ||
                channelInfo.lcd_info != null ||
                channelInfo.lighting_modes.length > 0
            ) {
                continue
            }
            const channelSettings = deviceSettings.sensorsAndChannels.get(channelName)
            for (const channel of device.status.channels) {
                if (channel.name !== channelName) {
                    continue
                }
                if (channel.duty != null) {
                    if (!includesLoads && channel.name.endsWith('Load')) continue
                    if (!includedDuties && !channel.name.endsWith('Load')) continue
                    // handles both duty and load
                    const [min, max, avg, count] = calcMinMaxAvg(
                        channel.name,
                        device.status_history,
                        DataType.DUTY,
                    )
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        dataType: DataType.DUTY,
                        value: channel.duty,
                        min: min,
                        max: max,
                        avg: avg,
                        count: count,
                    })
                }
                if (includesRPMs && channel.rpm != null) {
                    const [min, max, avg, count] = calcMinMaxAvg(
                        channel.name,
                        device.status_history,
                        DataType.RPM,
                    )
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        dataType: DataType.RPM,
                        value: channel.rpm,
                        min: min,
                        max: max,
                        avg: avg,
                        count: count,
                    })
                }
                if (includesFreqs && channel.freq != null) {
                    let [min, max, avg, count] = calcMinMaxAvg(
                        channel.name,
                        device.status_history,
                        DataType.FREQ,
                    )
                    if (settingsStore.frequencyPrecision > 1) {
                        min = min / settingsStore.frequencyPrecision
                        max = max / settingsStore.frequencyPrecision
                        avg = avg / settingsStore.frequencyPrecision
                    }
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        dataType: DataType.FREQ,
                        value: channel.freq / settingsStore.frequencyPrecision,
                        min: min,
                        max: max,
                        avg: avg,
                        count: count,
                    })
                }
                if (includesWatts && channel.watts != null) {
                    let [min, max, avg, count] = calcMinMaxAvg(
                        channel.name,
                        device.status_history,
                        DataType.WATTS,
                    )
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        dataType: DataType.WATTS,
                        value: channel.watts,
                        min: min,
                        max: max,
                        avg: avg,
                        count: count,
                    })
                }
            }
        }
    }
}

initTableData()

// Allows us to efficiently calculate averages in real time
const calcCumulativeAverage = (row: DeviceData, newValue: number, newCount: number): number =>
    (row.avg * row.count + newValue) / newCount

const updateTableData = () => {
    for (const row of deviceTableData.value) {
        let newValue: number
        switch (row.dataType) {
            case DataType.TEMP:
                newValue = Number(
                    deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!.temp,
                )
                break
            case DataType.DUTY:
                newValue = Number(
                    deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!.duty,
                )
                break
            case DataType.RPM:
                newValue = Number(
                    deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!.rpm,
                )
                break
            case DataType.FREQ:
                newValue =
                    Number(
                        deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!
                            .freq,
                    ) / settingsStore.frequencyPrecision
                break
            case DataType.WATTS:
                newValue = Number(
                    deviceStore.currentDeviceStatus.get(row.deviceUID)!.get(row.channelID)!.watts,
                )
                break
            default:
                newValue = 0
        }
        row.value = newValue
        row.min = Math.min(row.min, newValue)
        row.max = Math.max(row.max, newValue)
        const newCount = row.count + 1
        row.avg = calcCumulativeAverage(row, newValue, newCount)
        row.count = newCount
    }
}

const format = (value: number, dataType: DataType): string => {
    if (dataType === DataType.TEMP || dataType === DataType.WATTS) {
        return value.toFixed(1)
    } else if (dataType === DataType.FREQ && settingsStore.frequencyPrecision > 1) {
        return value.toFixed(2)
    } else {
        return value.toFixed(0)
    }
}
const suffix = (dataType: DataType): string => {
    switch (dataType) {
        case DataType.TEMP:
            return ' °'
        case DataType.DUTY:
            return ' %'
        case DataType.RPM:
            return ' rpm'
        case DataType.FREQ:
            return settingsStore.frequencyPrecision === 1 ? ' Mhz' : ' Ghz'
        case DataType.WATTS:
            return ' W'
        default:
            return ' %'
    }
}
const suffixStyle = (dataType: DataType): string => {
    switch (dataType) {
        case DataType.TEMP:
            return ''
        case DataType.FREQ:
        case DataType.WATTS:
            return 'font-size: 0.62rem'
        default:
            return 'font-size: 0.7rem'
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
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="pb-24 h-screen w-full">
            <div class="h-full">
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
                    <Column field="value" :header="t('components.sensorTable.current')">
                        <template #body="slotProps">
                            {{ format(slotProps.data.value, slotProps.data.dataType) }}
                            <span :style="suffixStyle(slotProps.data.dataType)">{{
                                suffix(slotProps.data.dataType)
                            }}</span>
                        </template>
                    </Column>
                    <Column field="min" :header="t('components.sensorTable.min')">
                        <template #body="slotProps">
                            {{ format(slotProps.data.min, slotProps.data.dataType) }}
                            <span :style="suffixStyle(slotProps.data.dataType)">{{
                                suffix(slotProps.data.dataType)
                            }}</span>
                        </template>
                    </Column>
                    <Column field="max" :header="t('components.sensorTable.max')">
                        <template #body="slotProps">
                            {{ format(slotProps.data.max, slotProps.data.dataType) }}
                            <span :style="suffixStyle(slotProps.data.dataType)">{{
                                suffix(slotProps.data.dataType)
                            }}</span>
                        </template>
                    </Column>
                    <Column field="avg" :header="t('components.sensorTable.average')">
                        <template #body="slotProps">
                            {{ format(slotProps.data.avg, slotProps.data.dataType) }}
                            <span :style="suffixStyle(slotProps.data.dataType)">{{
                                suffix(slotProps.data.dataType)
                            }}</span>
                        </template>
                    </Column>
                </DataTable>
            </div>
        </ScrollAreaViewport>
        <ScrollAreaScrollbar
            class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
            orientation="vertical"
        >
            <ScrollAreaThumb
                class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
            />
        </ScrollAreaScrollbar>
    </ScrollAreaRoot>
</template>

<style lang="scss" scoped></style>
