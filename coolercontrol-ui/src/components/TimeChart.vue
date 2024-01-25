<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { onMounted, watch } from 'vue'
import { type Color, Device } from '@/models/Device'
import uPlot from 'uplot'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const uSeriesData: uPlot.AlignedData = []
const uLineNames: Array<string> = []

interface Props {
    temp: boolean
    load: boolean
    duty: boolean
    rpm: boolean
}

const props = defineProps<Props>()

interface DeviceLineProperties {
    color: Color
    hidden: boolean
}

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
    const currentStatusLength = settingsStore.systemOverviewOptions.selectedTimeRange.seconds
    const uTimeData = new Uint32Array(currentStatusLength)
    for (const [statusIndex, status] of firstDevice.status_history
        .slice(-currentStatusLength)
        .entries()) {
        uTimeData[statusIndex] = Math.floor(new Date(status.timestamp).getTime() / 1000) // Status' Unix timestamp
    }

    // line values should not be greater than 100 and not less than 0,
    //  but we need to use decimal values for at least temps, so Float32.
    // TypedArrays have a fixed length, so we need to manage this ourselves
    const uLineData = new Map<string, Float32Array>()

    for (const device of deviceStore.allDevices()) {
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        for (const [statusIndex, status] of device.status_history
            .slice(-currentStatusLength)
            .entries()) {
            for (const tempStatus of status.temps) {
                if (!props.temp) {
                    break
                }
                const tempSettings = deviceSettings.sensorsAndChannels.get(tempStatus.name)!
                const lineName = createLineName(device, tempStatus.name)
                if (!uLineNames.includes(lineName)) {
                    uLineNames.push(lineName)
                }
                if (!allDevicesLineProperties.has(lineName)) {
                    allDevicesLineProperties.set(lineName, {
                        color: tempSettings.color,
                        hidden: tempSettings.hide,
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
                if (channelStatus.duty != null) {
                    if (
                        (props.load && channelStatus.name.endsWith('Load')) ||
                        (props.duty && !channelStatus.name.endsWith('Load'))
                    ) {
                        // check for null or undefined
                        const channelSettings = deviceSettings.sensorsAndChannels.get(
                            channelStatus.name,
                        )!
                        const lineName = createLineName(device, channelStatus.name)
                        if (!uLineNames.includes(lineName)) {
                            uLineNames.push(lineName)
                        }
                        if (!allDevicesLineProperties.has(lineName)) {
                            allDevicesLineProperties.set(lineName, {
                                color: channelSettings.color,
                                hidden: channelSettings.hide,
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
                if (props.rpm && channelStatus.rpm != null) {
                    // check for null or undefined
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
                        })
                    }
                    let floatArray = uLineData.get(lineName)
                    if (floatArray == null) {
                        floatArray = new Float32Array(currentStatusLength)
                        uLineData.set(lineName, floatArray)
                    }
                    floatArray[statusIndex] = channelStatus.rpm
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
    const currentStatusLength = settingsStore.systemOverviewOptions.selectedTimeRange.seconds
    shiftSeriesData(1)

    const newTimestamp = firstDevice.status.timestamp
    uSeriesData[0][currentStatusLength - 1] = Math.floor(new Date(newTimestamp).getTime() / 1000)

    for (const device of deviceStore.allDevices()) {
        const newStatus = device.status
        for (const tempStatus of newStatus.temps) {
            if (!props.temp) {
                break
            }
            const lineName = createLineName(device, tempStatus.name)
            uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] = tempStatus.temp
        }
        for (const channelStatus of newStatus.channels) {
            if (channelStatus.duty != null) {
                if (
                    (props.load && channelStatus.name.endsWith('Load')) ||
                    (props.duty && !channelStatus.name.endsWith('Load'))
                ) {
                    // check for null or undefined
                    const lineName = createLineName(device, channelStatus.name)
                    uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] =
                        channelStatus.duty
                }
            }
            if (props.rpm && channelStatus.rpm != null) {
                // check for null or undefined
                const lineName = createLineName(device, channelStatus.name + '_rpm')
                uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] =
                    channelStatus.rpm
            }
        }
    }
    console.debug('Updated uPlot Data')
}

const callRefreshSeriesListData = () => {
    // we use a wrapper function here so we can easily update the
    // function reference after the onMount() below
    refreshSeriesListData()
}

let refreshSeriesListData = () => {
    initUSeriesData()
}

initUSeriesData()

const uPlotSeries: Array<uPlot.Series> = [{}]

const getLineStyle = (lineName: string): Array<number> => {
    const lineLower = lineName.toLowerCase()
    if (lineLower.endsWith('rpm')) {
        return [1, 1]
    } else if (lineLower.includes('fan')) {
        return [10, 3, 2, 3]
    } else if (lineLower.includes('load') || lineLower.includes('pump')) {
        return [6, 3]
    } else {
        return []
    }
}
for (const lineName of uLineNames) {
    if (lineName.endsWith('_rpm')) {
        uPlotSeries.push({
            show: !allDevicesLineProperties.get(lineName)?.hidden,
            label: lineName,
            scale: 'rpm',
            auto: true,
            stroke: allDevicesLineProperties.get(lineName)?.color,
            points: {
                show: false,
            },
            dash: getLineStyle(lineName),
            spanGaps: true,
            width: settingsStore.systemOverviewOptions.timeChartLineScale,
            // min: 0,
            // max: 10000,
            value: (_, rawValue) => (rawValue != null ? rawValue.toFixed(0) : rawValue),
        })
        continue
    } else {
        uPlotSeries.push({
            show: !allDevicesLineProperties.get(lineName)?.hidden,
            label: lineName,
            scale: '%',
            auto: false,
            stroke: allDevicesLineProperties.get(lineName)?.color,
            points: {
                show: false,
            },
            dash: getLineStyle(lineName),
            spanGaps: true,
            width: settingsStore.systemOverviewOptions.timeChartLineScale,
            min: 0,
            max: 100,
            value: (_, rawValue) => (rawValue != null ? rawValue.toFixed(1) : rawValue),
        })
    }
}

const hourFormat = settingsStore.time24 ? 'H' : 'h'
const uOptions: uPlot.Options = {
    width: 200,
    height: 200,
    select: {
        show: false,
        left: 0,
        top: 0,
        width: 0,
        height: 0,
    },
    series: uPlotSeries,
    axes: [
        {
            stroke: colors.themeColors().text_title,
            size: Math.max(deviceStore.getREMSize(2.0), 34), // seems to be the magic amount
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            ticks: {
                show: true,
                stroke: colors.themeColors().text_title,
                width: 1,
                size: 5,
            },
            space: deviceStore.getREMSize(6.25),
            incrs: [15, 60, 300],
            values: [
                // min tick incr | default | year | month | day | hour | min | sec | mode
                [300, `{${hourFormat}}:{mm}`, null, null, null, null, null, null, 0],
                [60, `{${hourFormat}}:{mm}`, null, null, null, null, null, null, 0],
                [15, `{${hourFormat}}:{mm}:{ss}`, null, null, null, null, null, null, 0],
            ],
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors().text_title,
            },
            grid: {
                show: true,
                stroke: colors.themeColors().text_description,
                width: 1,
                dash: [1, 3],
            },
        },
        {
            scale: '%',
            label: '',
            stroke: colors.themeColors().text_title,
            size: deviceStore.getREMSize(2.5),
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            gap: 3,
            ticks: {
                show: true,
                stroke: colors.themeColors().text_title,
                width: 1,
                size: 5,
            },
            incrs: [10],
            values: (_, ticks) => ticks.map((rawValue) => rawValue + '°/%'),
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors().text_title,
            },
            grid: {
                show: true,
                stroke: colors.themeColors().text_description,
                width: 1,
                dash: [1, 3],
            },
        },
        {
            side: 1,
            scale: 'rpm',
            label: '',
            stroke: colors.themeColors().text_title,
            size: deviceStore.getREMSize(2.5),
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            incrs: [200, 1000],
            // values: (_, ticks) => ticks.map((rawValue) => rawValue + ' rpm'),
            // gap: 3,
            // ticks: {
            //     show: true,
            //     stroke: colors.themeColors().text_title,
            //     width: 1,
            //     size: 5,
            // },
            // border: {
            //     show: true,
            //     width: 1,
            //     stroke: colors.themeColors().text_title,
            // },
            grid: {
                show: false,
            },
        },
    ],
    scales: {
        '%': {
            auto: false,
            range: [0, 100],
        },
        rpm: {
            auto: true,
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
        show: false,
        // focus: {
        //   prox: 10,
        // }
    },
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
                if (onlyRecentStatus) {
                    updateUSeriesData()
                } else {
                    initUSeriesData() // reinit everything
                }
                uPlotChart.setData(uSeriesData)
            })
        } else if (name === 'loadCompleteStatusHistory') {
            after(() => {
                console.warn('Complete Status History loaded')
                initUSeriesData()
                uPlotChart.setData(uSeriesData)
            })
        }
    })

    watch(settingsStore.systemOverviewOptions, () => {
        callRefreshSeriesListData()
        for (const [index, _lineName] of uLineNames.entries()) {
            const seriesIndex = index + 1
            uPlotSeries[seriesIndex].width = settingsStore.systemOverviewOptions.timeChartLineScale
            uPlotChart.delSeries(seriesIndex)
            uPlotChart.addSeries(uPlotSeries[seriesIndex], seriesIndex)
        }
        uPlotChart.redraw()
    })

    watch(settingsStore.allUIDeviceSettings, () => {
        // re-set all line colors on device settings change
        for (const device of deviceStore.allDevices()) {
            const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
            for (const tempStatus of device.status.temps) {
                if (!props.temp) {
                    break
                }
                allDevicesLineProperties.set(createLineName(device, tempStatus.name), {
                    color: deviceSettings.sensorsAndChannels.get(tempStatus.name)!.color,
                    hidden: deviceSettings.sensorsAndChannels.get(tempStatus.name)!.hide,
                })
            }
            for (const channelStatus of device.status.channels) {
                if (channelStatus.duty != null) {
                    if (
                        (props.load && channelStatus.name.endsWith('Load')) ||
                        (props.duty && !channelStatus.name.endsWith('Load'))
                    ) {
                        // check for null or undefined
                        allDevicesLineProperties.set(createLineName(device, channelStatus.name), {
                            color: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.color,
                            hidden: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.hide,
                        })
                    }
                }
                if (props.rpm && channelStatus.rpm != null) {
                    // check for null or undefined
                    allDevicesLineProperties.set(
                        createLineName(device, channelStatus.name + '_rpm'),
                        {
                            color: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.color,
                            hidden: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.hide,
                        },
                    )
                }
            }
        }
        for (const [index, lineName] of uLineNames.entries()) {
            const seriesIndex = index + 1
            uPlotSeries[seriesIndex].show = !allDevicesLineProperties.get(lineName)?.hidden
            uPlotSeries[seriesIndex].stroke = allDevicesLineProperties.get(lineName)?.color
            uPlotChart.delSeries(seriesIndex)
            uPlotChart.addSeries(uPlotSeries[seriesIndex], seriesIndex)
        }
        uPlotChart.redraw()
    })
})
</script>

<template>
    <div id="u-plot-chart" class="chart"></div>
</template>

<style scoped>
.chart {
    width: 100%;
    height: calc(100vh - 11.2rem);
}
</style>
