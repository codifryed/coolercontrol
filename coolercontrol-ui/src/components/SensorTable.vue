<!--
  SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiMemory, mdiMinusThick } from '@mdi/js'
import UiTable from '@/shell/ui/UiTable.vue'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { computed, Ref, ref, watch } from 'vue'
import { Dashboard, DataType } from '@/models/Dashboard.ts'
import { UID } from '@/models/Device.ts'
import type { ChannelStats } from '@/models/Stats'
import {
    formatStatValue,
    lifetimeStatsOf,
    lifetimeToDisplay,
    statUnitSuffix,
    toDisplayUnits,
} from '@/components/chartStats.ts'
import { useLifetimeStats } from '@/composables/useLifetimeStats.ts'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'reka-ui'
import { useI18n } from 'vue-i18n'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()
const { stats, reset } = useLifetimeStats()

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
    // True when the next row belongs to a different channel (or this is the
    // last row). Used to drop the channel column's border-b between same-
    // channel rows so they read as one merged cell. Set by a post-pass in
    // rebuildTableData after sort.
    isLastOfChannel?: boolean
}

// Device subheader color: matches the per-device userColor used in AppTreeMenu.
// Empty string means "inherit theme default" so the device name stays readable
// when no color has been set.
const deviceColor = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.userColor ?? ''

// A fan channel can have both duty and rpm in the same physical channel, which
// produces multiple rows that share the same rowID (device.uid + channel.name).
// Render the channel marker + label only on the first such row so multi-status
// channels read as one logical group (matches the old rowspan-grouped layout).
const isFirstRowOfChannel = (index: number): boolean => {
    if (index === 0) return true
    return deviceTableData.value[index - 1].rowID !== deviceTableData.value[index].rowID
}

// Drops the channel column's border-b between same-channel rows. Combined with
// the marker/label hiding above, the duty + rpm rows of one channel read as a
// single merged cell on the left.
const rowClass = (data: DeviceData): string =>
    data.isLastOfChannel === false ? 'channel-continued' : ''

// First row of each device group gets a full-width device header row above it.
const isFirstOfDevice = (index: number): boolean =>
    index === 0 ||
    deviceTableData.value[index - 1].deviceUID !== deviceTableData.value[index].deviceUID

const rebuildTableData = () => {
    deviceTableData.value.length = 0
    for (const device of deviceStore.allDevices()) {
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        if (!includesDevice(device.uid)) continue
        const row = (channelID: string, dataType: DataType): DeviceData => {
            const channelSettings = deviceSettings.sensorsAndChannels.get(channelID)
            return {
                rowID: device.uid + channelID,
                deviceUID: device.uid,
                deviceName: deviceSettings.name,
                channelID: channelID,
                channelColor: channelSettings?.color ?? 'white',
                channelLabel: channelSettings?.name ?? channelID,
                dataType: dataType,
            }
        }
        for (const temp of device.status.temps) {
            if (!includesDeviceChannel(device.uid, temp.name) || !includesTemps) continue
            deviceTableData.value.push(row(temp.name, DataType.TEMP))
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
            for (const channel of device.status.channels) {
                if (channel.name !== channelName) continue
                if (channel.duty != null) {
                    if (!includesLoads && channel.name.endsWith('Load')) continue
                    if (!includedDuties && !channel.name.endsWith('Load')) continue
                    deviceTableData.value.push(row(channel.name, DataType.DUTY))
                }
                if (includesRPMs && channel.rpm != null) {
                    deviceTableData.value.push(row(channel.name, DataType.RPM))
                }
                if (includesFreqs && channel.freq != null) {
                    deviceTableData.value.push(row(channel.name, DataType.FREQ))
                }
                if (includesWatts && channel.watts != null) {
                    deviceTableData.value.push(row(channel.name, DataType.WATTS))
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
    // Mark whether each row is the last of its channel group, so the channel
    // column's bottom border can be suppressed between same-channel rows.
    for (let i = 0; i < deviceTableData.value.length; i++) {
        const next = deviceTableData.value[i + 1]
        deviceTableData.value[i].isLastOfChannel =
            next == null || next.rowID !== deviceTableData.value[i].rowID
    }
}

// Exposed so parent views can wire a "Reset" button into their existing
// control panel (where the chart-type Select and filter dropdowns live).
const resetStats = (): Promise<void> => reset()
defineExpose({ resetStats })

rebuildTableData()

const currentValue = (row: DeviceData): number => {
    const values = deviceStore.currentDeviceStatus.get(row.deviceUID)?.get(row.channelID)
    switch (row.dataType) {
        case DataType.TEMP:
            return Number(values?.temp)
        case DataType.DUTY:
            return Number(values?.duty)
        case DataType.RPM:
            return Number(values?.rpm)
        case DataType.FREQ:
            return toDisplayUnits(
                Number(values?.freq),
                DataType.FREQ,
                settingsStore.frequencyPrecision,
            )
        case DataType.WATTS:
            return Number(values?.watts)
        default:
            return 0
    }
}

interface RowValues {
    current: number
    // Null until the daemon has observed this line.
    stats: ChannelStats | null
}

// Recomputed on every status tick: the lifetime stats are folded and re-triggered each tick.
const rowValues = computed<Array<RowValues>>(() =>
    deviceTableData.value.map((row) => ({
        current: currentValue(row),
        stats: lifetimeToDisplay(
            lifetimeStatsOf(stats.value, row.deviceUID, row.channelID, row.dataType),
            row.dataType,
            settingsStore.frequencyPrecision,
        ),
    })),
)

const format = (value: number, dataType: DataType): string =>
    formatStatValue(value, dataType, settingsStore.frequencyPrecision)
const suffix = (dataType: DataType): string =>
    statUnitSuffix(dataType, settingsStore.frequencyPrecision, t)
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

// Settings changed (name/color/label); rebuild the rows.
watch(settingsStore.allUIDeviceSettings, () => rebuildTableData())
</script>

<template>
    <ScrollAreaRoot class="h-full" style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="h-full w-full">
            <div class="h-full">
                <UiTable sticky-header>
                    <template #head>
                        <tr>
                            <th>{{ t('components.sensorTable.channel') }}</th>
                            <th class="w-[12%]">{{ t('components.sensorTable.current') }}</th>
                            <th class="w-[18%]">
                                <span class="ml-10">{{ t('components.sensorTable.range') }}</span>
                            </th>
                            <th class="w-[12%]">{{ t('components.sensorTable.average') }}</th>
                        </tr>
                    </template>
                    <template v-for="(row, index) in deviceTableData" :key="index">
                        <tr v-if="isFirstOfDevice(index)" class="group-header">
                            <td colspan="4">
                                <div
                                    class="flex items-center font-semibold text-lg"
                                    :style="{ color: deviceColor(row.deviceUID) }"
                                >
                                    <div class="mr-2">
                                        <svg-icon
                                            type="mdi"
                                            :path="mdiMemory"
                                            :size="deviceStore.getREMSize(1.5)"
                                        />
                                    </div>
                                    <div>{{ row.deviceName }}</div>
                                </div>
                            </td>
                        </tr>
                        <tr :class="rowClass(row)">
                            <td>
                                <div
                                    v-if="isFirstRowOfChannel(index)"
                                    class="flex items-center gap-2"
                                >
                                    <svg-icon
                                        type="mdi"
                                        :path="mdiMinusThick"
                                        :size="14"
                                        class="ml-1 shrink-0"
                                        :style="{ color: row.channelColor }"
                                    />
                                    {{ row.channelLabel }}
                                </div>
                            </td>
                            <td>
                                <span class="font-bold">{{
                                    format(rowValues[index].current, row.dataType)
                                }}</span>
                                <span :style="suffixStyle(row.dataType)">{{
                                    suffix(row.dataType)
                                }}</span>
                            </td>
                            <td>
                                <span
                                    v-if="rowValues[index].stats == null"
                                    class="text-text-color-secondary"
                                    >-</span
                                >
                                <span
                                    v-else
                                    class="inline-flex items-baseline font-numeric tabular-nums"
                                >
                                    <span class="text-right min-w-[3rem]">{{
                                        format(rowValues[index].stats!.min, row.dataType)
                                    }}</span>
                                    <span class="mx-2 text-text-color-secondary">-</span>
                                    <span class="text-left min-w-[3rem]">{{
                                        format(rowValues[index].stats!.max, row.dataType)
                                    }}</span>
                                    <span class="ml-1" :style="suffixStyle(row.dataType)">{{
                                        suffix(row.dataType)
                                    }}</span>
                                </span>
                            </td>
                            <td>
                                <span
                                    v-if="rowValues[index].stats == null"
                                    class="text-text-color-secondary"
                                    >-</span
                                >
                                <template v-else>
                                    {{ format(rowValues[index].stats!.avg, row.dataType) }}
                                    <span :style="suffixStyle(row.dataType)">{{
                                        suffix(row.dataType)
                                    }}</span>
                                </template>
                            </td>
                        </tr>
                    </template>
                </UiTable>
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

<style lang="scss" scoped>
// Device group header rows: a heavier bottom border + top spacing so adjacent
// device groups read as separate sections.
tr.group-header > td {
    border-bottom: 3px solid rgb(var(--colors-border-one));
    padding-top: 2rem;
}

// Drop the Channel cell's bottom border between rows of the same channel
// (e.g. a fan with both duty and rpm). Combined with hiding the marker/label
// on subsequent rows, the cells visually fuse into one tall "rowspan" cell.
tr.channel-continued > td:first-of-type {
    border-bottom: 0;
}

// Row hover: subtle backdrop on the row's cells. For multi-row channels
// (e.g. fan duty + rpm), hovering either row highlights both so the pair
// reads as one selection. The reverse direction needs :has() (Chromium
// 105+); older Qt builds get only forward pairing.
tbody tr:not(.group-header):hover > td,
tbody tr.channel-continued:hover + tr > td,
tbody tr.channel-continued:has(+ tr:hover) > td {
    background-color: rgba(var(--colors-surface-hover) / 0.05);
}
</style>
