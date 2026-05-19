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
import { mdiMemory, mdiRestart } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import Button from 'primevue/button'
import { onBeforeUnmount, onMounted, Ref, ref, watch } from 'vue'
import { Dashboard, DataType } from '@/models/Dashboard.ts'
import { UID } from '@/models/Device.ts'
import {
    ChannelStats,
    DeviceStatsDTO,
    StatsResponseDTO,
    defaultStatsResponse,
} from '@/models/Stats'
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
const currentStats: Ref<StatsResponseDTO> = ref(defaultStatsResponse())

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
    count: number // number of values folded so far
}

// Daemon hasn't observed this entry yet. Sentinel min lets the first Math.min
// in updateTableData seed the correct minimum; max=avg=count=0 self-seed too.
const emptyStats = (): { min: number; max: number; avg: number; count: number } => ({
    min: Number.MAX_SAFE_INTEGER,
    max: 0,
    avg: 0,
    count: 0,
})

const fromChannelStats = (
    stats: ChannelStats | undefined,
): { min: number; max: number; avg: number; count: number } => {
    if (stats == null || stats.count === 0) return emptyStats()
    return { min: stats.min, max: stats.max, avg: stats.avg, count: stats.count }
}

const rebuildTableData = (stats: StatsResponseDTO) => {
    deviceTableData.value.length = 0
    const statsByUid = new Map<UID, DeviceStatsDTO>()
    for (const d of stats.devices) statsByUid.set(d.uid, d)

    for (const device of deviceStore.allDevices()) {
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        if (!includesDevice(device.uid)) continue
        const deviceStats = statsByUid.get(device.uid)
        for (const temp of device.status.temps) {
            if (!includesDeviceChannel(device.uid, temp.name) || !includesTemps) continue
            const channelSettings = deviceSettings.sensorsAndChannels.get(temp.name)
            deviceTableData.value.push({
                rowID: device.uid + temp.name,
                deviceUID: device.uid,
                deviceName: deviceSettings.name,
                channelID: temp.name,
                channelColor: channelSettings?.color ?? 'white',
                dataType: DataType.TEMP,
                channelLabel: channelSettings?.name ?? temp.name,
                value: temp.temp,
                ...fromChannelStats(deviceStats?.temps?.[temp.name]),
            })
        }
        if (device.info == null) continue
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (
                !includesDeviceChannel(device.uid, channelName) ||
                channelInfo.lcd_info != null ||
                channelInfo.lighting_modes.length > 0
            ) {
                continue
            }
            const channelSettings = deviceSettings.sensorsAndChannels.get(channelName)
            const channelDeviceStats = deviceStats?.channels?.[channelName]
            for (const channel of device.status.channels) {
                if (channel.name !== channelName) continue
                if (channel.duty != null) {
                    if (!includesLoads && channel.name.endsWith('Load')) continue
                    if (!includedDuties && !channel.name.endsWith('Load')) continue
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        dataType: DataType.DUTY,
                        value: channel.duty,
                        ...fromChannelStats(channelDeviceStats?.['DUTY']),
                    })
                }
                if (includesRPMs && channel.rpm != null) {
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        dataType: DataType.RPM,
                        value: channel.rpm,
                        ...fromChannelStats(channelDeviceStats?.['RPM']),
                    })
                }
                if (includesFreqs && channel.freq != null) {
                    const scaled = fromChannelStats(channelDeviceStats?.['FREQ'])
                    if (settingsStore.frequencyPrecision > 1 && scaled.count > 0) {
                        scaled.min /= settingsStore.frequencyPrecision
                        scaled.max /= settingsStore.frequencyPrecision
                        scaled.avg /= settingsStore.frequencyPrecision
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
                        ...scaled,
                    })
                }
                if (includesWatts && channel.watts != null) {
                    deviceTableData.value.push({
                        rowID: device.uid + channel.name,
                        deviceUID: device.uid,
                        deviceName: deviceSettings.name,
                        channelID: channel.name,
                        channelColor: channelSettings?.color ?? 'white',
                        channelLabel: channelSettings?.name ?? channel.name,
                        dataType: DataType.WATTS,
                        value: channel.watts,
                        ...fromChannelStats(channelDeviceStats?.['WATTS']),
                    })
                }
            }
        }
    }
    if (settingsStore.menuOrder.length > 0) {
        deviceTableData.value.sort((a, b) => {
            const getDeviceIndex = (item: DeviceData) => {
                const index = settingsStore.menuOrder.findIndex(
                    (menuItem) => menuItem.id === item.deviceUID,
                )
                return index >= 0 ? index : Number.MAX_SAFE_INTEGER
            }
            const deviceCompare = getDeviceIndex(a) - getDeviceIndex(b)
            if (deviceCompare !== 0) return deviceCompare

            const deviceMenuOrderItem = settingsStore.menuOrder.find(
                (item) => item.id === a.deviceUID,
            )
            if (deviceMenuOrderItem?.children?.length) {
                const getChannelIndex = (item: DeviceData) => {
                    const index = deviceMenuOrderItem.children.indexOf(
                        `${item.deviceUID}_${item.channelID}`,
                    )
                    return index >= 0 ? index : Number.MAX_SAFE_INTEGER
                }
                return getChannelIndex(a) - getChannelIndex(b)
            } else {
                return 0
            }
        })
    }
}

const refreshStats = async () => {
    currentStats.value = await deviceStore.daemonClient.getStats()
    rebuildTableData(currentStats.value)
}

const resetStats = async () => {
    currentStats.value = await deviceStore.daemonClient.resetStats()
    rebuildTableData(currentStats.value)
}

// Initial render before /stats returns: build rows with empty baselines so the
// table is laid out immediately. refreshStats() below replaces them with the
// daemon-provided values.
rebuildTableData(currentStats.value)

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
            return ` ${t('common.tempUnit')}`
        case DataType.DUTY:
            return ` ${t('common.percentUnit')}`
        case DataType.RPM:
            return ` ${t('common.rpmAbbr')}`
        case DataType.FREQ:
            return settingsStore.frequencyPrecision === 1
                ? ` ${t('common.mhzAbbr')}`
                : ` ${t('common.ghzAbbr')}`
        case DataType.WATTS:
            return ` ${t('common.wattAbbr')}`
        default:
            return ` ${t('common.percentUnit')}`
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

const onVisibilityChange = () => {
    if (document.visibilityState === 'visible') {
        refreshStats()
    }
}

onMounted(async () => {
    await refreshStats()

    deviceStore.$onAction(({ name, after }) => {
        if (name === 'updateStatus') {
            after((onlyRecentStatus: boolean) => {
                if (onlyRecentStatus) {
                    updateTableData()
                } else {
                    // Full history reload (e.g. daemon reconnect): daemon stats
                    // were not affected, but resync to be safe.
                    refreshStats()
                }
            })
        }
    })

    watch(settingsStore.allUIDeviceSettings, () => {
        // Settings changed (name/color/label); rebuild rows with cached stats.
        rebuildTableData(currentStats.value)
    })

    document.addEventListener('visibilitychange', onVisibilityChange)
})

onBeforeUnmount(() => {
    document.removeEventListener('visibilitychange', onVisibilityChange)
})
</script>

<template>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="pb-24 h-screen w-full">
            <div class="h-full">
                <div class="flex justify-end px-2 py-1">
                    <Button
                        text
                        size="small"
                        @click="resetStats"
                        v-tooltip.left="t('components.sensorTable.resetStatsTooltip')"
                    >
                        <svg-icon type="mdi" :path="mdiRestart" :size="deviceStore.getREMSize(1)" />
                        <span class="ml-1">{{ t('components.sensorTable.resetStats') }}</span>
                    </Button>
                </div>
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
