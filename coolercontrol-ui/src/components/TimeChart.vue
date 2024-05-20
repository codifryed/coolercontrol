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
import { onMounted, watch } from 'vue'
import { type Color, Device } from '@/models/Device'
import uPlot from 'uplot'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const uSeriesData: uPlot.AlignedData = []
const uLineNames: Array<string> = []
const SCALE_KEY_PERCENT: string = '%'
const SCALE_KEY_RPM: string = 'rpm'

interface Props {
    temp: boolean
    load: boolean
    duty: boolean
    rpm: boolean
    freq: boolean
}

const props = defineProps<Props>()

interface DeviceLineProperties {
    color: Color
    hidden: boolean
    name: string
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
                if (channelStatus.duty != null) {
                    if (
                        (props.load && channelStatus.name.endsWith('Load')) ||
                        (props.duty && !channelStatus.name.endsWith('Load'))
                    ) {
                        // check for null or undefined
                        const channelSettings = deviceSettings.sensorsAndChannels.get(
                            channelStatus.name,
                        )!
                        const lineName = createLineName(device, channelStatus.name + '_load')
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
                            name: channelSettings.name,
                        })
                    }
                    let floatArray = uLineData.get(lineName)
                    if (floatArray == null) {
                        floatArray = new Float32Array(currentStatusLength)
                        uLineData.set(lineName, floatArray)
                    }
                    floatArray[statusIndex] = channelStatus.rpm
                }
                if (props.freq && channelStatus.freq != null) {
                    // check for null or undefined
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
                    floatArray[statusIndex] = channelStatus.freq
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
            const lineName = createLineName(device, tempStatus.name + '_temp')
            uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] = tempStatus.temp
        }
        for (const channelStatus of newStatus.channels) {
            if (channelStatus.duty != null) {
                if (
                    (props.load && channelStatus.name.endsWith('Load')) ||
                    (props.duty && !channelStatus.name.endsWith('Load'))
                ) {
                    // check for null or undefined
                    const lineName = createLineName(device, channelStatus.name + '_load')
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
            if (props.freq && channelStatus.freq != null) {
                // check for null or undefined
                const lineName = createLineName(device, channelStatus.name + '_freq')
                uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] =
                    channelStatus.freq
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
    if (lineLower.endsWith('rpm') || lineLower.endsWith('freq')) {
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
    if (lineName.endsWith('_rpm') || lineName.endsWith('_freq')) {
        uPlotSeries.push({
            show: !allDevicesLineProperties.get(lineName)?.hidden,
            label: lineName,
            scale: SCALE_KEY_RPM,
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
            // value: (_, rawValue) => (rawValue != null ? rawValue.toFixed(0) : rawValue),
        })
    } else {
        uPlotSeries.push({
            show: !allDevicesLineProperties.get(lineName)?.hidden,
            label: lineName,
            scale: SCALE_KEY_PERCENT,
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

const tooltipPlugin = () => {
    const tooltip = document.createElement('div')
    tooltip.className = 'u-plot-tooltip'
    tooltip.style.display = 'none'
    tooltip.style.position = 'absolute'
    tooltip.style.background = 'color-mix(in srgb, var(--surface-card) 94%, transparent)'
    tooltip.style.border = '2px solid var(--cc-bg-three)'
    tooltip.style.borderRadius = 'var(--border-radius)'
    tooltip.style.color = 'var(--text-color)'
    tooltip.style.padding = 'var(--inline-spacing)'
    let tooltipVisible: boolean = false

    const showTooltip = () => {
        if (!tooltipVisible) {
            tooltip.style.display = 'block'
            tooltipVisible = true
        }
    }

    const hideTooltip = () => {
        if (tooltipVisible) {
            tooltip.style.display = 'none'
            tooltipVisible = false
        }
    }

    const setTooltip = (u: uPlot, contentHTML: string) => {
        if (
            u.cursor.top == null ||
            u.cursor.top < 0 ||
            u.cursor.left == null ||
            u.cursor.left < 0
        ) {
            hideTooltip()
            return
        }
        tooltip.innerHTML = contentHTML
        showTooltip() // need to show the tooltip before getting the element's size
        // handle tooltip positioning:
        const leftOffset =
            u.cursor.left! < u.width * 0.9 - tooltip.offsetWidth ? 10 : -(tooltip.offsetWidth + 10)
        const topOffset =
            u.cursor.top! < u.height * 0.9 - tooltip.offsetHeight
                ? 10
                : -(tooltip.offsetHeight + 10)
        tooltip.style.top = topOffset + u.cursor.top + 'px'
        tooltip.style.left = leftOffset + u.cursor.left + 'px'
    }

    return {
        hooks: {
            init: [
                (u: uPlot, _: uPlot.Options, __: uPlot.AlignedData) => {
                    u.over.appendChild(tooltip)
                },
            ],
            setCursor: [
                (u: uPlot) => {
                    const seriesTexts: Array<string> = []
                    const c = u.cursor
                    let lowerPercentLimit: number = 110
                    let upperPercentLimit: number = -1
                    const rpmScaleMax: undefined | number = u.scales[SCALE_KEY_RPM].max
                    let lowerRpmLimit: number = 10_000
                    let upperRpmLimit: number = -1
                    for (const [i, series] of u.series.entries()) {
                        if (i == 0) {
                            // time series
                            continue
                        }
                        if (series.show) {
                            const seriesValue: number = u.data[i][c.idx!]!
                            if (seriesValue == null) {
                                // when leaving the canvas area during an update,
                                // the value can be undefined
                                continue
                            }
                            const isPercentScale: boolean = series.scale == SCALE_KEY_PERCENT
                            // Calculate Cursor values once for all series:
                            if (isPercentScale && upperPercentLimit == -1) {
                                const percentCursorValue = u.posToVal(c.top ?? 0, SCALE_KEY_PERCENT)
                                lowerPercentLimit = percentCursorValue - 2
                                upperPercentLimit = percentCursorValue + 2
                                if (lowerPercentLimit < 2) {
                                    // keeps upper and lower boundaries within the canvas area
                                    lowerPercentLimit = 0
                                    upperPercentLimit = 4
                                } else if (upperPercentLimit > 98) {
                                    lowerPercentLimit = 96
                                    upperPercentLimit = 120
                                }
                            } else if (
                                !isPercentScale &&
                                upperRpmLimit == -1 &&
                                rpmScaleMax != null
                            ) {
                                // Calculate RPM series' value range once for all series
                                const rpmCursorValue = u.posToVal(c.top ?? 0, SCALE_KEY_RPM)
                                const rpmSeriesValueRange = rpmScaleMax * 0.04
                                const rpmSeriesValueRangeSplit = rpmSeriesValueRange / 2
                                lowerRpmLimit = rpmCursorValue - rpmSeriesValueRangeSplit
                                upperRpmLimit = rpmCursorValue + rpmSeriesValueRangeSplit
                                if (lowerRpmLimit < rpmSeriesValueRangeSplit) {
                                    // keeps upper and lower boundaries within the canvas area
                                    lowerRpmLimit = 0
                                    upperRpmLimit = rpmSeriesValueRange
                                } else if (upperRpmLimit > rpmScaleMax - rpmSeriesValueRangeSplit) {
                                    lowerRpmLimit = rpmScaleMax - rpmSeriesValueRange
                                    upperRpmLimit = rpmScaleMax
                                }
                            }
                            // Check if series is in range of the cursor
                            if (
                                isPercentScale &&
                                (seriesValue < lowerPercentLimit || seriesValue > upperPercentLimit)
                            ) {
                                // Out of range for the percent scale
                                continue
                            }
                            if (
                                !isPercentScale &&
                                (seriesValue < lowerRpmLimit || seriesValue > upperRpmLimit)
                            ) {
                                // out of range for the rpm scale
                                continue
                            }
                            const lineName = allDevicesLineProperties.get(series.label!)?.name
                            const lineValue: string =
                                series.label!.endsWith('fan') || series.label!.endsWith('temp')
                                    ? seriesValue.toFixed(1)
                                    : seriesValue.toString()
                            let suffix = ''
                            if (series.label!.endsWith('temp')) {
                                suffix = '°'
                            } else if (series.label!.endsWith('rpm')) {
                                suffix = 'rpm'
                            } else if (series.label!.endsWith('freq')) {
                                suffix = 'mhz'
                            } else {
                                suffix = '%'
                            }
                            const lineColor = allDevicesLineProperties.get(series.label!)?.color
                            seriesTexts.push(
                                `<tr><td><i class="pi pi-minus" style="color:${lineColor};"/></td><td>${lineName}&nbsp;</td><td>${lineValue} ${suffix}</td></tr>`,
                            )
                        }
                    }
                    if (seriesTexts.length > 0) {
                        seriesTexts.splice(0, 0, '<table>')
                        seriesTexts.push('</table>')
                        setTooltip(u, seriesTexts.join(''))
                    } else {
                        hideTooltip()
                    }
                },
            ],
        },
    }
}

const columnHighlightPlugin = () => {
    const highlightEl: HTMLElement = document.createElement('div')
    const highlightEl2: HTMLElement = document.createElement('div')
    return {
        hooks: {
            init: [
                (u: uPlot) => {
                    const underEl: HTMLElement = u.under
                    const overEl: HTMLElement = u.over
                    uPlot.assign(highlightEl.style, {
                        pointerEvents: 'none',
                        display: 'none',
                        position: 'absolute',
                        left: 0,
                        top: 0,
                        height: '4%',
                        backgroundColor:
                            'color-mix(in srgb, var(--primary-color) 30%, transparent)',
                    })
                    uPlot.assign(highlightEl2.style, {
                        pointerEvents: 'none',
                        display: 'none',
                        position: 'absolute',
                        left: 0,
                        top: 0,
                        height: '100%',
                        backgroundColor:
                            'color-mix(in srgb, var(--primary-color) 10%, transparent)',
                    })
                    underEl.appendChild(highlightEl)
                    underEl.appendChild(highlightEl2)
                    // show/hide highlight on enter/exit
                    overEl.addEventListener('mouseenter', () => {
                        highlightEl.style.display = 'block'
                        highlightEl2.style.display = 'block'
                    })
                    overEl.addEventListener('mouseleave', () => {
                        highlightEl.style.display = 'none'
                        highlightEl2.style.display = 'none'
                    })
                },
            ],
            setCursor: [
                (u: uPlot) => {
                    const currIdx = u.cursor.idx ?? 0
                    const [iMin, iMax] = u.series[0].idxs!
                    const dx = iMax - iMin
                    const width = u.bbox.width / dx / devicePixelRatio
                    const xVal = u.scales.x.distr == 2 ? currIdx : u.data[0][currIdx]
                    const left = u.valToPos(xVal, 'x') - width / 2

                    highlightEl.style.transform = 'translateX(' + Math.round(left) + 'px)'
                    highlightEl2.style.transform = 'translateX(' + Math.round(left) + 'px)'
                    highlightEl.style.width = Math.round(Math.max(width, 2)) + 'px'
                    highlightEl2.style.width = Math.round(Math.max(width, 2)) + 'px'

                    const percentCursorValue = u.posToVal(u.cursor.top ?? 0, SCALE_KEY_PERCENT)
                    const topCursorValue = Math.min(Math.max(percentCursorValue + 2, 4), 100)
                    const topCursorPos = u.valToPos(topCursorValue, SCALE_KEY_PERCENT)
                    highlightEl.style.top = topCursorPos + 'px'
                },
            ],
        },
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
            stroke: colors.themeColors.text_color,
            size: Math.max(deviceStore.getREMSize(2.0), 34), // seems to be the magic amount
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            ticks: {
                show: true,
                stroke: colors.themeColors.text_color,
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
                stroke: colors.themeColors.text_color,
            },
            grid: {
                show: true,
                stroke: colors.themeColors.gray_600,
                width: 1,
                dash: [1, 3],
            },
        },
        {
            scale: SCALE_KEY_PERCENT,
            label: 'duty %  |  temperature °C',
            labelGap: 0,
            labelSize: deviceStore.getREMSize(1.3),
            labelFont: `bold ${deviceStore.getREMSize(1.0)}px sans-serif`,
            stroke: colors.themeColors.text_color,
            size: deviceStore.getREMSize(2.5),
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            gap: 3,
            ticks: {
                show: true,
                stroke: colors.themeColors.text_color,
                width: 1,
                size: 5,
            },
            incrs: [10],
            // values: (_, ticks) => ticks.map((rawValue) => rawValue + '°/%'),
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors.text_color,
            },
            grid: {
                show: true,
                stroke: colors.themeColors.gray_600,
                width: 1,
                dash: [1, 3],
            },
        },
        {
            side: 1,
            scale: SCALE_KEY_RPM,
            label: 'rpm  |  mhz',
            labelGap: deviceStore.getREMSize(1.6),
            labelSize: deviceStore.getREMSize(2.7),
            labelFont: `bold ${deviceStore.getREMSize(1.0)}px sans-serif`,
            stroke: colors.themeColors.text_color,
            size: deviceStore.getREMSize(2.5),
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            ticks: {
                show: true,
                stroke: colors.themeColors.surface_card,
                width: 1,
                size: 5,
            },
            incrs: (_self: uPlot, _axisIdx: number, _scaleMin: number, scaleMax: number) => {
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
            },
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
            // @ts-ignore
            range: (_self, _dataMin, dataMax) => {
                if (!props.rpm && !props.freq) {
                    return [null, null]
                }
                const [min, max] = uPlot.rangeNum(0, dataMax || 90.5, 0.1, true)
                return [min, Math.min(max!, 10_000)]
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
        }
    })

    watch(settingsStore.systemOverviewOptions, () => {
        callRefreshSeriesListData()
        uPlotChart.setData(uSeriesData)
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
                    name: deviceSettings.sensorsAndChannels.get(tempStatus.name)!.name,
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
                            name: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.name,
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
                            name: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.name,
                        },
                    )
                } else if (props.freq && channelStatus.freq != null) {
                    // check for null or undefined
                    allDevicesLineProperties.set(
                        createLineName(device, channelStatus.name + '_freq'),
                        {
                            color: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.color,
                            hidden: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.hide,
                            name: deviceSettings.sensorsAndChannels.get(channelStatus.name)!.name,
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
