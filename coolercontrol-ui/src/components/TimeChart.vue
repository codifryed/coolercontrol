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
import { onMounted } from 'vue'
import { Device, UID } from '@/models/Device'
import uPlot from 'uplot'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import {
    columnHighlightPlugin,
    DeviceLineProperties,
    mouseWheelZoomPlugin,
    SCALE_KEY_PERCENT,
    SCALE_KEY_RPM,
    tooltipPlugin,
} from '@/components/u-plot-plugins.ts'
import { Dashboard, DataType } from '@/models/Dashboard.ts'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
// const yCrosshair = computed(
//     () =>
//         `${settingsStore.systemOverviewOptions.timeChartLineScale}px solid color-mix(in srgb, var(--primary-color) 30%, transparent)`,
// )
const colors = useThemeColorsStore()
const uSeriesData: uPlot.AlignedData = []
const uLineNames: Array<string> = []

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
const timeRangeSeconds = props.dashboard.timeRangeSeconds

const allDevicesLineProperties = new Map<string, DeviceLineProperties>()

/**
 * Line Names should be unique for our Series Data.
 * @param device
 * @param statusName
 */
const createLineName = (device: Device, statusName: string): string =>
    `${device.type}_${device.type_index}_${statusName}`

/**
 * Converts our internal Device objects and statuses into the format required by uPlot
 */
const initUSeriesData = () => {
    uSeriesData.length = 0
    uLineNames.length = 0

    const firstDevice: Device = deviceStore.allDevices().next().value
    const currentStatusLength = timeRangeSeconds / settingsStore.ccSettings.poll_rate
    const uTimeData = new Float64Array(currentStatusLength)
    for (const [statusIndex, status] of firstDevice.status_history
        .slice(-currentStatusLength)
        .entries()) {
        uTimeData[statusIndex] = new Date(status.timestamp).getTime() / 1000 // Status' Unix timestamp
    }

    // We need to use decimal values for at least temps, so Float32.
    // TypedArrays have a fixed length, so we need to manage this ourselves
    const uLineData = new Map<string, Float32Array>()

    for (const device of deviceStore.allDevices()) {
        if (!includesDevice(device.uid)) continue
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        for (const [statusIndex, status] of device.status_history
            .slice(-currentStatusLength)
            .entries()) {
            for (const tempStatus of status.temps) {
                if (!includesTemps) break
                if (!includesDeviceChannel(device.uid, tempStatus.name)) continue
                const tempSettings = deviceSettings.sensorsAndChannels.get(tempStatus.name)!
                const lineName = createLineName(device, tempStatus.name + '_temp')
                if (!uLineNames.includes(lineName)) {
                    uLineNames.push(lineName)
                }
                if (!allDevicesLineProperties.has(lineName)) {
                    allDevicesLineProperties.set(lineName, {
                        color: tempSettings.color,
                        hidden: tempSettings.hide,
                        name: tempSettings.name,
                    })
                }
                let floatArray = uLineData.get(lineName)
                if (floatArray == null) {
                    floatArray = new Float32Array(currentStatusLength)
                    uLineData.set(lineName, floatArray)
                }
                floatArray[statusIndex] = tempStatus.temp
            }
            for (const channelStatus of status.channels) {
                if (!includesDeviceChannel(device.uid, channelStatus.name)) continue
                if (channelStatus.duty != null) {
                    const isLoadChannel = includesLoads && channelStatus.name.endsWith('Load')
                    const isFanDutyChannel = includedDuties && !channelStatus.name.endsWith('Load')
                    if (isLoadChannel || isFanDutyChannel) {
                        const channelSettings = deviceSettings.sensorsAndChannels.get(
                            channelStatus.name,
                        )!
                        const lineNameExt: string = isLoadChannel ? '_load' : '_duty'
                        const lineName = createLineName(device, channelStatus.name + lineNameExt)
                        if (!uLineNames.includes(lineName)) {
                            uLineNames.push(lineName)
                        }
                        if (!allDevicesLineProperties.has(lineName)) {
                            allDevicesLineProperties.set(lineName, {
                                color: channelSettings.color,
                                hidden: channelSettings.hide,
                                name: channelSettings.name,
                            })
                        }
                        let floatArray = uLineData.get(lineName)
                        if (floatArray == null) {
                            floatArray = new Float32Array(currentStatusLength)
                            uLineData.set(lineName, floatArray)
                        }
                        floatArray[statusIndex] = channelStatus.duty
                    }
                }
                if (includesRPMs && channelStatus.rpm != null) {
                    const channelSettings = deviceSettings.sensorsAndChannels.get(
                        channelStatus.name,
                    )!
                    const lineName = createLineName(device, channelStatus.name + '_rpm')
                    if (!uLineNames.includes(lineName)) {
                        uLineNames.push(lineName)
                    }
                    if (!allDevicesLineProperties.has(lineName)) {
                        allDevicesLineProperties.set(lineName, {
                            color: channelSettings.color,
                            hidden: channelSettings.hide,
                            name: channelSettings.name,
                        })
                    }
                    let floatArray = uLineData.get(lineName)
                    if (floatArray == null) {
                        floatArray = new Float32Array(currentStatusLength)
                        uLineData.set(lineName, floatArray)
                    }
                    floatArray[statusIndex] = channelStatus.rpm / settingsStore.frequencyPrecision
                }
                if (includesFreqs && channelStatus.freq != null) {
                    const channelSettings = deviceSettings.sensorsAndChannels.get(
                        channelStatus.name,
                    )!
                    const lineName = createLineName(device, channelStatus.name + '_freq')
                    if (!uLineNames.includes(lineName)) {
                        uLineNames.push(lineName)
                    }
                    if (!allDevicesLineProperties.has(lineName)) {
                        allDevicesLineProperties.set(lineName, {
                            color: channelSettings.color,
                            hidden: channelSettings.hide,
                            name: channelSettings.name,
                        })
                    }
                    let floatArray = uLineData.get(lineName)
                    if (floatArray == null) {
                        floatArray = new Float32Array(currentStatusLength)
                        uLineData.set(lineName, floatArray)
                    }
                    floatArray[statusIndex] = channelStatus.freq / settingsStore.frequencyPrecision
                }
            }
        }
    }

    for (const lineName of uLineNames) {
        // the uLineNames Array keeps our LineData arrays in order
        uSeriesData.push(uLineData.get(lineName)!)
    }
    uSeriesData.splice(0, 0, uTimeData) // 'inserts' time values as the first array, where uPlot expects it
    console.debug('Initialized uPlot Series Data')
}

const shiftSeriesData = (shiftLength: number) => {
    for (const arr of uSeriesData) {
        for (let i = 0; i < arr.length - shiftLength; i++) {
            arr[i] = arr[i + shiftLength] // Shift left
        }
    }
}

const updateUSeriesData = () => {
    const firstDevice: Device = deviceStore.allDevices().next().value
    const currentStatusLength = timeRangeSeconds / settingsStore.ccSettings.poll_rate
    shiftSeriesData(1)

    const newTimestamp = firstDevice.status.timestamp
    uSeriesData[0][currentStatusLength - 1] = new Date(newTimestamp).getTime() / 1000

    for (const device of deviceStore.allDevices()) {
        if (!includesDevice(device.uid)) continue
        const newStatus = device.status
        for (const tempStatus of newStatus.temps) {
            if (!includesTemps) break
            if (!includesDeviceChannel(device.uid, tempStatus.name)) continue
            const lineName = createLineName(device, tempStatus.name + '_temp')
            uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] = tempStatus.temp
        }
        for (const channelStatus of newStatus.channels) {
            if (!includesDeviceChannel(device.uid, channelStatus.name)) continue
            if (channelStatus.duty != null) {
                const isLoadChannel = includesLoads && channelStatus.name.endsWith('Load')
                const isFanDutyChannel = includedDuties && !channelStatus.name.endsWith('Load')
                if (isLoadChannel || isFanDutyChannel) {
                    const lineNameExt: string = isLoadChannel ? '_load' : '_duty'
                    const lineName = createLineName(device, channelStatus.name + lineNameExt)
                    uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] =
                        channelStatus.duty
                }
            }
            if (includesRPMs && channelStatus.rpm != null) {
                const lineName = createLineName(device, channelStatus.name + '_rpm')
                uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] =
                    channelStatus.rpm / settingsStore.frequencyPrecision
            }
            if (includesFreqs && channelStatus.freq != null) {
                const lineName = createLineName(device, channelStatus.name + '_freq')
                uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] =
                    channelStatus.freq / settingsStore.frequencyPrecision
            }
        }
    }
    console.debug('Updated uPlot Data')
}

// const callRefreshSeriesListData = () => {
//     // we use a wrapper function here so we can easily update the
//     // function reference after the onMount() below
//     refreshSeriesListData()
// }

// @ts-ignore
let refreshSeriesListData = () => {
    initUSeriesData()
}

initUSeriesData()

const uPlotSeries: Array<uPlot.Series> = [{}]

const getLineStyle = (lineName: string): Array<number> => {
    const lineLower = lineName.toLowerCase()
    if (lineLower.endsWith('rpm') || lineLower.endsWith('freq')) {
        return [1, 1]
    } else if (lineLower.endsWith('load') || lineLower.includes('pump')) {
        return [6, 3]
    } else if (lineLower.endsWith('duty')) {
        return [10, 3, 2, 3]
    } else {
        return []
    }
}

let hasDegreeAxis: boolean = false
let hasFrequencyAxis: boolean = false
for (const lineName of uLineNames) {
    if (lineName.endsWith('_rpm') || lineName.endsWith('_freq')) {
        hasFrequencyAxis = true
        uPlotSeries.push({
            show: !allDevicesLineProperties.get(lineName)?.hidden,
            label: lineName,
            scale: SCALE_KEY_RPM,
            auto: props.dashboard.autoScaleFrequency,
            stroke: allDevicesLineProperties.get(lineName)?.color,
            points: {
                show: false,
            },
            dash: getLineStyle(lineName),
            spanGaps: true,
            width: settingsStore.chartLineScale,
            // min: 0,
            // max: 10000,
            // value: (_, rawValue) => (rawValue != null ? rawValue.toFixed(0) : rawValue),
            // value: (_, rawValue) => {
            //     // if (props.dashboard.frequencyPrecision === 1)
            //     //     return rawValue != null ? rawValue.toFixed(0) : rawValue
            //     // else
            //         return rawValue != null ? (rawValue / 1000).toFixed(1) : rawValue
            // },
        })
    } else {
        hasDegreeAxis = true
        uPlotSeries.push({
            show: !allDevicesLineProperties.get(lineName)?.hidden,
            label: lineName,
            scale: SCALE_KEY_PERCENT,
            auto: props.dashboard.autoScaleDegree,
            stroke: allDevicesLineProperties.get(lineName)?.color,
            points: {
                show: false,
            },
            dash: getLineStyle(lineName),
            spanGaps: true,
            width: settingsStore.chartLineScale,
            // min: 0,
            // max: 100,
            value: (_, rawValue) => (rawValue != null ? rawValue.toFixed(1) : rawValue),
        })
    }
}

const hourFormat = settingsStore.time24 ? 'H' : 'h'
const uOptions: uPlot.Options = {
    width: 200,
    height: 200,
    series: uPlotSeries,
    axes: [
        {
            stroke: colors.themeColors.text_color,
            size: deviceStore.isSafariWebKit()
                ? Math.max(deviceStore.getREMSize(2.0), 38)
                : deviceStore.getREMSize(2.0),
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            ticks: {
                show: true,
                stroke: colors.themeColors.text_color,
                width: 1,
                size: 5,
            },
            space: deviceStore.getREMSize(6.25),
            incrs: [15, 60, 300, 900],
            values: [
                // min tick incr | default | year | month | day | hour | min | sec | mode
                [900, `{${hourFormat}}:{mm}`, null, null, null, null, null, null, 0],
                [300, `{${hourFormat}}:{mm}`, null, null, null, null, null, null, 0],
                [60, `{${hourFormat}}:{mm}`, null, null, null, null, null, null, 0],
                [15, `{${hourFormat}}:{mm}:{ss}`, null, null, null, null, null, null, 0],
            ],
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors.text_color_secondary,
            },
            grid: {
                show: true,
                stroke: colors.themeColors.bg_two,
                width: 1,
                // dash: [1, 3],
            },
        },
        {
            scale: SCALE_KEY_PERCENT,
            label: '%  /  °C',
            labelGap: 0,
            labelSize: deviceStore.getREMSize(1.4),
            labelFont: `sans-serif`,
            stroke: colors.themeColors.text_color,
            size: deviceStore.getREMSize(2.5),
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            gap: 3,
            ticks: {
                show: true,
                stroke: colors.themeColors.text_color_secondary,
                width: 1,
                size: 5,
            },
            incrs: [10],
            // values: (_, ticks) => ticks.map((rawValue) => rawValue + '°/%'),
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors.text_color_secondary,
            },
            grid: {
                show: true,
                stroke: colors.themeColors.bg_two,
                width: 1,
                // dash: [1, 3],
            },
        },
        {
            side: 1,
            scale: SCALE_KEY_RPM,
            label: settingsStore.frequencyPrecision === 1 ? 'rpm / Mhz' : 'krpm / Ghz',
            labelGap: settingsStore.frequencyPrecision === 1 ? deviceStore.getREMSize(1.6) : 0,
            labelSize:
                settingsStore.frequencyPrecision === 1
                    ? deviceStore.getREMSize(2.9)
                    : deviceStore.getREMSize(1.4),
            // labelFont, unlike font, seems to take rem values properly, and by is 1rem by default:
            labelFont: `sans-serif`,
            stroke: colors.themeColors.text_color,
            size: deviceStore.getREMSize(2.5),
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            ticks: {
                show: true,
                stroke: colors.themeColors.text_color_secondary,
                width: 1,
                size: 5,
            },
            values: (_, axisValues) =>
                axisValues.map((rawValue) =>
                    settingsStore.frequencyPrecision === 1
                        ? rawValue.toFixed(0)
                        : rawValue.toFixed(1),
                ),
            incrs: (_self: uPlot, _axisIdx: number, _scaleMin: number, scaleMax: number) => {
                if (settingsStore.frequencyPrecision === 1) {
                    if (scaleMax > 7000) {
                        return [1000]
                    } else if (scaleMax > 3000) {
                        return [500]
                    } else if (scaleMax > 1300) {
                        return [200]
                    } else if (scaleMax > 700) {
                        return [100]
                    } else {
                        return [50]
                    }
                } else {
                    return [1]
                }
            },
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors.text_color_secondary,
            },
            grid: {
                show: !hasDegreeAxis && hasFrequencyAxis,
                stroke: colors.themeColors.bg_two,
                width: 1,
                // dash: [1, 3],
            },
        },
    ],
    scales: {
        '%': {
            auto: props.dashboard.autoScaleDegree,
            range: (_self, _dataMin, dataMax) => {
                if (!hasDegreeAxis) return [null, null]
                return props.dashboard.autoScaleDegree
                    ? uPlot.rangeNum(0, dataMax || 10.5, 0.1, true)
                    : [props.dashboard.degreeMin, props.dashboard.degreeMax]
            },
        },
        rpm: {
            auto: props.dashboard.autoScaleFrequency,
            // @ts-ignore
            range: (_self, _dataMin, dataMax) => {
                if (!hasFrequencyAxis) return [null, null]
                return props.dashboard.autoScaleFrequency
                    ? uPlot.rangeNum(0, dataMax || 90.5, 0.1, true)
                    : [
                          props.dashboard.frequencyMin / settingsStore.frequencyPrecision,
                          props.dashboard.frequencyMax / settingsStore.frequencyPrecision,
                      ]
            },
        },
        x: {
            auto: true,
            time: true,
            // range: (min, max) => [uSeriesData[0].splice(-61)[0], uSeriesData[0].splice(-1)[0]],
            // range: timeRange(),
            // range: (min, max) => [((Date.now() / 1000) - 60), uPlotSeries[]],
        },
    },
    legend: {
        show: false,
    },
    cursor: {
        show: true,
        x: false,
        // enable for crosshair on y-axis (in addition to css properties):
        y: false,
        points: {
            show: false,
        },
        drag: {
            x: false,
            y: false,
        },
    },
    plugins: [
        tooltipPlugin(allDevicesLineProperties),
        columnHighlightPlugin(),
        mouseWheelZoomPlugin(),
    ],
}
console.debug('Processed status data for System Overview')

//----------------------------------------------------------------------------------------------------------------------

onMounted(async () => {
    const uChartElement: HTMLElement = document.getElementById('u-plot-chart') ?? new HTMLElement()
    const uPlotChart = new uPlot(uOptions, uSeriesData, uChartElement)
    const getChartSize = () => {
        const cwh = uChartElement.getBoundingClientRect()
        return { width: cwh.width, height: cwh.height }
    }
    let isZoomed: boolean = false
    uPlotChart.setSize(getChartSize())
    const resizeObserver = new ResizeObserver((_) => {
        uPlotChart.setSize(getChartSize())
    })
    resizeObserver.observe(uChartElement)

    refreshSeriesListData = () => {
        initUSeriesData()
        uPlotChart.setData(uSeriesData)
    }

    deviceStore.$onAction(({ name, after }) => {
        if (name === 'updateStatus') {
            after((onlyRecentStatus: boolean) => {
                // zoom handling:
                if (
                    uPlotChart.scales.x.min != uPlotChart.data[0].at(0) ||
                    uPlotChart.scales.x.max != uPlotChart.data[0].at(-1)
                ) {
                    isZoomed = true
                    return
                } else if (isZoomed) {
                    // zoom has been reset
                    isZoomed = false
                    initUSeriesData() // reinit everything
                }
                if (onlyRecentStatus) {
                    updateUSeriesData()
                } else {
                    initUSeriesData() // reinit everything
                }
                uPlotChart.setData(uSeriesData, true)
            })
        }
    })
})
</script>

<template>
    <div class="p-2">
        <div id="u-plot-chart" class="chart"></div>
    </div>
</template>

<style scoped>
.chart {
    width: 100%;
    height: calc(100vh - 5.5rem);
}

/** To add a crosshair to the y-axis:
.chart :deep(.u-hz .u-cursor-y) {
    border-bottom: v-bind(yCrosshair);
}
*/
</style>
