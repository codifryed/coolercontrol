<!--
  SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { onMounted, onUnmounted, watch } from 'vue'
import { Device, UID } from '@/models/Device'
import uPlot from 'uplot'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import {
    columnHighlightPlugin,
    DeviceLineProperties,
    mouseWheelZoomPlugin,
    limitLinesPlugin,
    type DrawnLimitLine,
    tooltipPlugin,
} from '@/components/u-plot-plugins.ts'
import {
    SCALE_KEY_PERCENT,
    SCALE_KEY_RPM,
    SCALE_KEY_WATTS,
    type ScaleKey,
} from '@/components/chartScales.ts'
import { limitColor, type LimitLine } from '@/components/channelAttributes.ts'
import { lineDataIndex, lineSetMatches } from '@/components/chartSeriesMapping.ts'
import {
    chartValueToDisplay,
    isSyntheticStatus,
    lineDash,
    sampleRange,
    windowStats,
    type LineKey,
    type WindowLineStats,
    type WindowStatsPayload,
} from '@/components/chartStats.ts'
import { Dashboard, DataType } from '@/models/Dashboard.ts'
import type { SensorAndChannelSettings } from '@/models/UISettings.ts'
import { useI18n } from 'vue-i18n'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()
// const yCrosshair = computed(
//     () =>
//         `${settingsStore.systemOverviewOptions.timeChartLineScale}px solid color-mix(in srgb, var(--primary-color) 30%, transparent)`,
// )
const colors = useThemeColorsStore()
const uSeriesData: uPlot.AlignedData = []
const uLineNames: Array<string> = []

interface Props {
    dashboard: Dashboard
    // Emit windowStats as the data or the visible range changes. Off unless a legend or panel
    // shows them, so embedded charts do no extra work.
    emitWindowStats?: boolean
    // Driver limits to draw as dashed lines; they arrive after mount and redraw in place.
    limitLines?: Array<LimitLine>
}

const props = defineProps<Props>()
// The chart is built once from the line set it finds at mount. When that set changes the
// only honest repair is a fresh chart, which the parent gives us by remounting this
// component, the same way it does for a dashboard settings change.
const emit = defineEmits<{
    (e: 'lineSetChanged'): void
    (e: 'windowStats', payload: WindowStatsPayload): void
}>()
let remountRequested: boolean = false
const requestRemount = (): void => {
    if (remountRequested) return
    remountRequested = true
    console.info('Chart line set changed, remounting the chart')
    emit('lineSetChanged')
}
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
const timeRangeSeconds = props.dashboard.timeRangeSeconds

const allDevicesLineProperties = new Map<string, DeviceLineProperties>()

const lineKeys = new Map<string, LineKey>()
// uMasks[i] is 1 where uSeriesData[i + 1] holds a real reading, 0 for zero-fill and gaps.
const uMasks: Array<Uint8Array> = []

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
    const builtLineNames = uLineNames.slice()
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
    const uMaskData = new Map<string, Uint8Array>()
    const record = (
        lineName: string,
        settings: SensorAndChannelSettings,
        key: LineKey,
        statusIndex: number,
        value: number,
        valid: boolean,
    ): void => {
        if (!uLineNames.includes(lineName)) {
            uLineNames.push(lineName)
        }
        if (!allDevicesLineProperties.has(lineName)) {
            allDevicesLineProperties.set(lineName, { color: settings.color, name: settings.name })
        }
        if (!lineKeys.has(lineName)) {
            lineKeys.set(lineName, key)
        }
        let floatArray = uLineData.get(lineName)
        let mask = uMaskData.get(lineName)
        if (floatArray == null || mask == null) {
            floatArray = new Float32Array(currentStatusLength)
            mask = new Uint8Array(currentStatusLength)
            uLineData.set(lineName, floatArray)
            uMaskData.set(lineName, mask)
        }
        floatArray[statusIndex] = value
        mask[statusIndex] = valid ? 1 : 0
    }

    for (const device of deviceStore.allDevices()) {
        if (!includesDevice(device.uid)) continue
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        for (const [statusIndex, status] of device.status_history
            .slice(-currentStatusLength)
            .entries()) {
            const valid = !isSyntheticStatus(status)
            for (const tempStatus of status.temps) {
                if (!includesTemps) break
                if (!includesDeviceChannel(device.uid, tempStatus.name)) continue
                record(
                    createLineName(device, tempStatus.name + '_temp'),
                    deviceSettings.sensorsAndChannels.get(tempStatus.name)!,
                    {
                        deviceUID: device.uid,
                        channelName: tempStatus.name,
                        dataType: DataType.TEMP,
                    },
                    statusIndex,
                    tempStatus.temp,
                    valid,
                )
            }
            for (const channelStatus of status.channels) {
                if (!includesDeviceChannel(device.uid, channelStatus.name)) continue
                const channelRecord = (suffix: string, dataType: DataType, value: number): void =>
                    record(
                        createLineName(device, channelStatus.name + suffix),
                        deviceSettings.sensorsAndChannels.get(channelStatus.name)!,
                        { deviceUID: device.uid, channelName: channelStatus.name, dataType },
                        statusIndex,
                        value,
                        valid,
                    )
                if (channelStatus.duty != null) {
                    const isLoadChannel = includesLoads && channelStatus.name.endsWith('Load')
                    const isFanDutyChannel = includedDuties && !channelStatus.name.endsWith('Load')
                    if (isLoadChannel) {
                        channelRecord('_load', DataType.LOAD, channelStatus.duty)
                    } else if (isFanDutyChannel) {
                        channelRecord('_duty', DataType.DUTY, channelStatus.duty)
                    }
                }
                if (includesRPMs && channelStatus.rpm != null) {
                    channelRecord(
                        '_rpm',
                        DataType.RPM,
                        channelStatus.rpm / settingsStore.frequencyPrecision,
                    )
                }
                if (includesFreqs && channelStatus.freq != null) {
                    channelRecord(
                        '_freq',
                        DataType.FREQ,
                        channelStatus.freq / settingsStore.frequencyPrecision,
                    )
                }
                if (includesWatts && channelStatus.watts != null) {
                    channelRecord('_watts', DataType.WATTS, channelStatus.watts)
                }
            }
        }
    }

    // The chart's series carry the label, unit, scale and colour each line is drawn with,
    // and they are matched to the data by position alone. Swapping in data for a different
    // set of lines would draw one channel's values as another channel: an rpm reading
    // under a duty's label and percent scale, say. Keep the data the chart was built for
    // and let the remount rebuild both together.
    if (chart != null && !lineSetMatches(builtLineNames, uLineNames)) {
        uLineNames.length = 0
        uLineNames.push(...builtLineNames)
        requestRemount()
        return
    }

    uSeriesData.length = 0
    uMasks.length = 0
    for (const lineName of uLineNames) {
        // the uLineNames Array keeps our LineData arrays in order
        uSeriesData.push(uLineData.get(lineName)!)
        uMasks.push(uMaskData.get(lineName)!)
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
    for (const mask of uMasks) {
        mask.copyWithin(0, shiftLength)
    }
}

const updateUSeriesData = () => {
    const firstDevice: Device = deviceStore.allDevices().next().value
    const currentStatusLength = timeRangeSeconds / settingsStore.ccSettings.poll_rate
    shiftSeriesData(1)

    const newTimestamp = firstDevice.status.timestamp
    uSeriesData[0][currentStatusLength - 1] = new Date(newTimestamp).getTime() / 1000
    // A line the new status does not report keeps its shifted value, but not as a reading.
    for (const mask of uMasks) {
        mask[currentStatusLength - 1] = 0
    }

    // Writes the latest value of one line, and asks for a remount for a line the chart was
    // not built with: a channel that has started reporting needs a series of its own before
    // it can be drawn, and until then its value has nowhere to go.
    const setLatest = (lineName: string, value: number, valid: boolean): void => {
        const dataIndex = lineDataIndex(uLineNames, lineName)
        if (dataIndex == null) {
            requestRemount()
            return
        }
        uSeriesData[dataIndex][currentStatusLength - 1] = value
        uMasks[dataIndex - 1][currentStatusLength - 1] = valid ? 1 : 0
    }

    for (const device of deviceStore.allDevices()) {
        if (!includesDevice(device.uid)) continue
        const newStatus = device.status
        const valid = !isSyntheticStatus(newStatus)
        for (const tempStatus of newStatus.temps) {
            if (!includesTemps) break
            if (!includesDeviceChannel(device.uid, tempStatus.name)) continue
            const lineName = createLineName(device, tempStatus.name + '_temp')
            setLatest(lineName, tempStatus.temp, valid)
        }
        for (const channelStatus of newStatus.channels) {
            if (!includesDeviceChannel(device.uid, channelStatus.name)) continue
            if (channelStatus.duty != null) {
                const isLoadChannel = includesLoads && channelStatus.name.endsWith('Load')
                const isFanDutyChannel = includedDuties && !channelStatus.name.endsWith('Load')
                if (isLoadChannel || isFanDutyChannel) {
                    const lineNameExt: string = isLoadChannel ? '_load' : '_duty'
                    const lineName = createLineName(device, channelStatus.name + lineNameExt)
                    setLatest(lineName, channelStatus.duty, valid)
                }
            }
            if (includesRPMs && channelStatus.rpm != null) {
                const lineName = createLineName(device, channelStatus.name + '_rpm')
                setLatest(lineName, channelStatus.rpm / settingsStore.frequencyPrecision, valid)
            }
            if (includesFreqs && channelStatus.freq != null) {
                const lineName = createLineName(device, channelStatus.name + '_freq')
                setLatest(lineName, channelStatus.freq / settingsStore.frequencyPrecision, valid)
            }
            if (includesWatts && channelStatus.watts != null) {
                const lineName = createLineName(device, channelStatus.name + '_watts')
                setLatest(lineName, channelStatus.watts, valid)
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

let chart: uPlot | null = null
let isZoomed: boolean = false
let rafId: number | null = null
let rafPaused: boolean = false // rAF stopped due to user interaction
let userZoomed: boolean = false // user has made a zoom selection
// New data arrives roughly once a second, so a full-canvas redraw on every display
// frame is wasted work. Throttle the scroll redraw to a fixed, lower cadence.
const rafFrameIntervalMs = 1000 / 30
let lastRafDrawMs = 0
// Skip animating while the chart is scrolled out of view (dashboards can stack
// several charts, only some of them visible at a time).
let chartOnScreen = true
let visibilityObserver: IntersectionObserver | null = null
let resizeObserver: ResizeObserver | null = null
const startRaf = () => {
    if (rafId !== null || chart === null || !chartOnScreen) return
    const animate = () => {
        rafId = requestAnimationFrame(animate)
        const nowMs = Date.now()
        if (nowMs - lastRafDrawMs < rafFrameIntervalMs) return
        lastRafDrawMs = nowMs
        const now = nowMs / 1000
        chart!.setData(uSeriesData, false)
        // Snap the right edge to a whole device-pixel boundary so the x-axis only
        // advances in integer-pixel steps. This stabilizes the dash/dot phase between
        // frames and eliminates the shaky/crawling appearance of dashed series.
        const pxPerSec = chart!.bbox.width / timeRangeSeconds
        const snappedNow = pxPerSec > 0 ? Math.round(now * pxPerSec) / pxPerSec : now
        chart!.setScale('x', { min: snappedNow - timeRangeSeconds, max: snappedNow })
    }
    rafId = requestAnimationFrame(animate)
}

const stopRaf = () => {
    if (rafId !== null) {
        cancelAnimationFrame(rafId)
        rafId = null
    }
}

// Window stats cover whatever the x scale shows: the time range, or the zoomed-in part of it.
const computeWindowStats = (): void => {
    if (chart == null || uSeriesData.length === 0) return
    const time = uSeriesData[0]
    const dataStart = time[0]
    const dataEnd = time[time.length - 1]
    const xMin = chart.scales.x.min ?? dataStart
    const xMax = chart.scales.x.max ?? dataEnd
    const precision = settingsStore.frequencyPrecision
    const toDisplay = (value: number, dataType: DataType): number =>
        chartValueToDisplay(value, dataType, precision)
    const lines: Array<WindowLineStats> = []
    for (const [index, lineName] of uLineNames.entries()) {
        const key = lineKeys.get(lineName)
        if (key == null) continue
        const values = uSeriesData[index + 1]
        const mask = uMasks[index]
        const stats = windowStats(time, values, mask, xMin, xMax)
        const last = values.length - 1
        lines.push({
            lineName,
            seriesIndex: index + 1,
            ...key,
            color: allDevicesLineProperties.get(lineName)?.color ?? '',
            label: allDevicesLineProperties.get(lineName)?.name ?? lineName,
            latest: mask[last] === 1 ? toDisplay(values[last], key.dataType) : null,
            stats:
                stats == null
                    ? null
                    : {
                          min: toDisplay(stats.min, key.dataType),
                          max: toDisplay(stats.max, key.dataType),
                          avg: toDisplay(stats.avg, key.dataType),
                          count: stats.count,
                          jitter:
                              stats.jitter == null ? null : toDisplay(stats.jitter, key.dataType),
                      },
        })
    }
    emit('windowStats', {
        lines,
        sampleRange: sampleRange(time, xMin, xMax),
        spanSeconds: xMax - xMin,
        zoomed: xMax - xMin < (dataEnd - dataStart) * 0.99,
        scaleRanges: currentScaleRanges(),
    })
}

// Data updates and scale changes arrive in bursts (setData fires setScale); one pass per frame.
let windowStatsRafId: number | null = null
const scheduleWindowStats = (): void => {
    if (!props.emitWindowStats || windowStatsRafId !== null) return
    windowStatsRafId = requestAnimationFrame(() => {
        windowStatsRafId = null
        computeWindowStats()
    })
}

// Highlights one line (dimming the rest), or restores all of them with null.
const focusLine = (seriesIndex: number | null): void => {
    chart?.setSeries(seriesIndex, { focus: true })
}
defineExpose({ focusLine })

const currentScaleRanges = (): Partial<Record<ScaleKey, [number, number]>> => {
    const ranges: Partial<Record<ScaleKey, [number, number]>> = {}
    for (const key of [SCALE_KEY_PERCENT, SCALE_KEY_RPM, SCALE_KEY_WATTS]) {
        const scale = chart?.scales[key]
        if (scale?.min != null && scale.max != null) ranges[key] = [scale.min, scale.max]
    }
    return ranges
}

const currentLimitLines = (): Array<DrawnLimitLine> =>
    (props.limitLines ?? []).map((line) => ({
        value: line.value,
        scale: line.scale,
        color: limitColor(line.severity, colors.themeColors),
        label: line.label,
    }))

// Auto-scaled rpm stretches to show the fan limits; a user-set range is left alone.
const highestRpmLimit = (): number => {
    let highest = 0
    for (const line of props.limitLines ?? []) {
        if (line.scale === SCALE_KEY_RPM) highest = Math.max(highest, line.value)
    }
    return highest
}

watch(
    () => props.limitLines,
    () => chart?.redraw(false, true),
)

// chartKey remounts this component on any dashboard/device settings change, so anything
// held past unmount is stranded for the life of the page. The observers are the retainers:
// they keep the chart element reachable, which keeps its listener closures, the uPlot
// instance and the series arrays alive.
onUnmounted(() => {
    stopRaf()
    if (windowStatsRafId !== null) cancelAnimationFrame(windowStatsRafId)
    visibilityObserver?.disconnect()
    visibilityObserver = null
    resizeObserver?.disconnect()
    resizeObserver = null
    // Releases uPlot's canvases and its own listeners, and removes the DOM subtree the
    // plugin listeners are bound to.
    chart?.destroy()
    chart = null
})

// @ts-ignore
let refreshSeriesListData = () => {
    initUSeriesData()
}

initUSeriesData()

const uPlotSeries: Array<uPlot.Series> = [{}]

let hasDegreeAxis: boolean = false
let hasFrequencyAxis: boolean = false
let hasWattsAxis: boolean = false
for (const lineName of uLineNames) {
    const key = lineKeys.get(lineName)
    const dash = key == null ? [] : lineDash(key)
    if (lineName.endsWith('_rpm') || lineName.endsWith('_freq')) {
        hasFrequencyAxis = true
        uPlotSeries.push({
            label: lineName,
            scale: SCALE_KEY_RPM,
            auto: props.dashboard.autoScaleFrequency,
            stroke: allDevicesLineProperties.get(lineName)?.color,
            points: {
                show: false,
            },
            dash,
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
    } else if (lineName.endsWith('_watts')) {
        hasWattsAxis = true
        uPlotSeries.push({
            label: lineName,
            scale: SCALE_KEY_WATTS,
            auto: props.dashboard.autoScaleWatts,
            stroke: allDevicesLineProperties.get(lineName)?.color,
            points: {
                show: false,
            },
            dash,
            spanGaps: true,
            width: settingsStore.chartLineScale,
        })
    } else {
        hasDegreeAxis = true
        uPlotSeries.push({
            label: lineName,
            scale: SCALE_KEY_PERCENT,
            auto: props.dashboard.autoScaleDegree,
            stroke: allDevicesLineProperties.get(lineName)?.color,
            points: {
                show: false,
            },
            dash,
            spanGaps: true,
            width: settingsStore.chartLineScale,
            // min: 0,
            // max: 100,
            value: (_, rawValue) => (rawValue != null ? rawValue.toFixed(1) : rawValue),
        })
    }
}

const hourFormat = settingsStore.time24 ? 'HH' : 'h'
// Escalating tick increments so axes still render values at small chart
// heights: uPlot picks the first increment whose ticks fit, so full-height
// rendering is unchanged while smaller charts fall back to coarser steps.
const incrSteps = (base: number): number[] => [base, base * 2, base * 2.5, base * 5, base * 10]
const uOptions: uPlot.Options = {
    width: 200,
    height: 200,
    pxAlign: 0,
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
                stroke: colors.themeColors.border,
                width: 1,
                dash: [1, 2],
            },
        },
        {
            scale: SCALE_KEY_PERCENT,
            label: `${t('common.percentUnit')}  /  ${t('common.tempUnit')}`,
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
            incrs: incrSteps(10),
            // values: (_, ticks) => ticks.map((rawValue) => rawValue + '°/%'),
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors.text_color_secondary,
            },
            grid: {
                show: true,
                stroke: colors.themeColors.border,
                width: 1,
                dash: [1, 2],
            },
        },
        {
            side: 1,
            scale: SCALE_KEY_RPM,
            label:
                settingsStore.frequencyPrecision === 1
                    ? t('components.axisOptions.rpmMhz')
                    : t('components.axisOptions.krpmGhz'),
            labelGap: deviceStore.getREMSize(1.0),
            // The band has to hold the gap plus the rotated title, or the title
            // spills into whatever axis comes next (the watts axis, when shown).
            labelSize: deviceStore.getREMSize(2.9),
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
                        return incrSteps(1000)
                    } else if (scaleMax > 3000) {
                        return incrSteps(500)
                    } else if (scaleMax > 1300) {
                        return incrSteps(200)
                    } else if (scaleMax > 700) {
                        return incrSteps(100)
                    } else {
                        return incrSteps(50)
                    }
                } else {
                    if (scaleMax > 2) {
                        return incrSteps(1)
                    } else if (scaleMax > 1) {
                        return incrSteps(0.5)
                    } else {
                        return incrSteps(0.2)
                    }
                }
            },
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors.text_color_secondary,
            },
            grid: {
                show: !hasDegreeAxis && hasFrequencyAxis,
                stroke: colors.themeColors.border,
                width: 1,
                dash: [1, 2],
            },
        },
        {
            side: 1,
            scale: SCALE_KEY_WATTS,
            label: t('components.axisOptions.watts'),
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

            incrs: (_self: uPlot, _axisIdx: number, _scaleMin: number, scaleMax: number) => {
                if (scaleMax > 7000) {
                    return incrSteps(1000)
                } else if (scaleMax > 3000) {
                    return incrSteps(500)
                } else if (scaleMax > 1200) {
                    return incrSteps(200)
                } else if (scaleMax > 500) {
                    return incrSteps(100)
                } else if (scaleMax > 180) {
                    return incrSteps(50)
                } else if (scaleMax > 120) {
                    return incrSteps(20)
                } else if (scaleMax > 50) {
                    return incrSteps(10)
                } else if (scaleMax > 23) {
                    return incrSteps(5)
                } else if (scaleMax > 13) {
                    return incrSteps(2)
                } else if (scaleMax > 7) {
                    return incrSteps(1)
                } else if (scaleMax > 1) {
                    return incrSteps(0.5)
                }
                return incrSteps(0.1)
            },
            // values: (_, ticks) => ticks.map((rawValue) => rawValue + ' W'),
            border: {
                show: true,
                width: 1,
                stroke: colors.themeColors.text_color_secondary,
            },
            grid: {
                show: !hasDegreeAxis && !hasFrequencyAxis && hasWattsAxis,
                stroke: colors.themeColors.border,
                width: 1,
                dash: [1, 2],
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
                    ? uPlot.rangeNum(
                          0,
                          Math.max(dataMax || 0, highestRpmLimit()) || 90.5,
                          0.1,
                          true,
                      )
                    : [
                          props.dashboard.frequencyMin / settingsStore.frequencyPrecision,
                          props.dashboard.frequencyMax / settingsStore.frequencyPrecision,
                      ]
            },
        },
        W: {
            auto: props.dashboard.autoScaleWatts,
            range: (_self, _dataMin, dataMax) => {
                if (!hasWattsAxis) return [null, null]
                return props.dashboard.autoScaleWatts
                    ? uPlot.rangeNum(0, dataMax || 10.5, 0.1, true)
                    : [props.dashboard.wattsMin, props.dashboard.wattsMax]
            },
        },
        x: {
            auto: true,
            time: true,
            // @ts-ignore
            range: (self, newMin, newMax) => {
                let curMin = self.scales.x.min
                let curMax = self.scales.x.max
                // prevent zooming in too far, < 10 seconds
                if (newMax - newMin < 10) return [curMin, curMax]
                return [newMin, newMax]
            },
        },
    },
    legend: {
        show: false,
    },
    // Only reached through focusLine: cursor proximity focus stays off (no cursor.focus.prox).
    focus: {
        alpha: 0.22,
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
            // enable zoom selection of x-axis
            x: true,
            y: false,
            // distance to cover before starting selection
            dist: 10,
        },
        bind: {
            // @ts-ignore
            mousedown: (u, targ, handler) => {
                return (e) => {
                    if (e.button == 0) {
                        if (e.ctrlKey) {
                            // no drag-selection when ctrl is pressed
                            return
                        }
                        const pxPerXUnitSecond = u.valToPos(1, 'x') - u.valToPos(0, 'x')
                        // 10 seconds max zoom-in
                        u.cursor.drag!.dist = pxPerXUnitSecond * 10
                        handler(e)
                    }
                }
            },
        },
    },
    plugins: [
        tooltipPlugin(allDevicesLineProperties, t, settingsStore.frequencyPrecision),
        columnHighlightPlugin(),
        mouseWheelZoomPlugin(),
        limitLinesPlugin(currentLimitLines, () =>
            colors.convertColorToRGBA(colors.themeColors.bg_one, 0.85),
        ),
    ],
    hooks: {
        setScale: [
            (u: uPlot, key: string) => {
                // The running scroll animation moves the scale every frame; status updates
                // already schedule stats then.
                if (key === 'x' && (rafId === null || rafPaused)) scheduleWindowStats()
                // Only handle user-driven scale changes. Internal uPlot calls during init,
                // data updates, and rAF-driven setScale are all silenced by the rafPaused guard.
                if (key !== 'x' || !settingsStore.eyeCandy || !rafPaused) return
                // Compare visible span to the actual data span with 1% tolerance.
                // This is robust against floating-point drift and the fact that data[0][0]
                // may be zero-padded at startup.
                const scaleSpan = u.scales.x.max! - u.scales.x.min!
                const dataSpan = u.data[0][u.data[0].length - 1] - u.data[0][0]
                if (scaleSpan >= dataSpan * 0.99) {
                    userZoomed = false
                    rafPaused = false
                    initUSeriesData() // re-sync data skipped while zoomed
                    startRaf()
                } else if (!userZoomed) {
                    // First scale change that is a real zoom-in (drag or wheel).
                    userZoomed = true
                }
            },
        ],
    },
}
console.debug('Processed status data for System Overview')

//----------------------------------------------------------------------------------------------------------------------

onMounted(async () => {
    const uChartElement: HTMLElement = document.getElementById('u-plot-chart') ?? new HTMLElement()
    chart = new uPlot(uOptions, uSeriesData, uChartElement)
    const getChartSize = () => {
        const cwh = uChartElement.getBoundingClientRect()
        return { width: cwh.width, height: cwh.height }
    }
    chart.setSize(getChartSize())
    resizeObserver = new ResizeObserver((_) => {
        chart?.setSize(getChartSize())
    })
    resizeObserver.observe(uChartElement)

    // Pause the scroll animation whenever the chart leaves the viewport, and
    // re-sync the (skipped) data before resuming when it returns.
    visibilityObserver = new IntersectionObserver(
        (entries) => {
            chartOnScreen = entries[0]?.isIntersecting ?? true
            if (!chartOnScreen) {
                stopRaf()
            } else if (settingsStore.eyeCandy && !rafPaused && rafId === null) {
                initUSeriesData()
                chart!.setData(uSeriesData, false)
                startRaf()
            }
        },
        { threshold: 0 },
    )
    visibilityObserver.observe(uChartElement)

    uChartElement.addEventListener('mousedown', (event: MouseEvent) => {
        if (
            settingsStore.eyeCandy &&
            rafId !== null &&
            (event.button === 0 || event.button === 2)
        ) {
            stopRaf()
            rafPaused = true
        }
    })
    // capture: true fires during the capture phase (top-down), before the wheel zoom
    // plugin's listener on the child u.over element (bubbling phase).
    uChartElement.addEventListener(
        'wheel',
        (event: WheelEvent) => {
            if (!event.ctrlKey) return
            if (settingsStore.eyeCandy && rafId !== null) {
                if (
                    chart!.scales.x.min != chart!.data[0][0] ||
                    chart!.scales.x.max != chart!.data[0][chart!.data[0].length - 1]
                ) {
                    stopRaf()
                    rafPaused = true
                }
            }
        },
        { capture: true, passive: true },
    )
    uChartElement.addEventListener('mouseup', () => {
        if (settingsStore.eyeCandy && rafPaused) {
            // Defer: uPlot listens on document for mouseup to finalize zoom, so setScale
            // fires after this element listener. Check userZoomed only after that happens.
            setTimeout(() => {
                if (rafPaused && !userZoomed) {
                    rafPaused = false
                    initUSeriesData() // re-sync data skipped while paused
                    startRaf()
                }
            }, 0)
        }
    })
    if (settingsStore.eyeCandy) {
        startRaf()
    }

    refreshSeriesListData = () => {
        initUSeriesData()
        chart!.setData(uSeriesData)
    }
    scheduleWindowStats()
    deviceStore.$onAction(({ name, after }) => {
        if (name === 'updateStatus') {
            after((onlyRecentStatus: boolean) => {
                if (!settingsStore.eyeCandy) {
                    // Non-animated mode: zoom detection via scale vs data bounds comparison
                    if (
                        chart!.scales.x.min != chart!.data[0][0] ||
                        chart!.scales.x.max != chart!.data[0][chart!.data[0].length - 1]
                    ) {
                        isZoomed = true
                        return
                    } else if (isZoomed) {
                        // zoom has been reset
                        isZoomed = false
                        initUSeriesData() // reinit everything
                    }
                }
                if (onlyRecentStatus) {
                    if (settingsStore.eyeCandy && rafId === null) {
                        // eyeCandy with rAF paused (user zoomed): do not shift data,
                        // as it causes the viewport contents to drift under the
                        // fixed zoom scale.
                        return
                    }
                    updateUSeriesData()
                } else {
                    initUSeriesData() // reinit everything
                }
                if (!settingsStore.eyeCandy) {
                    chart!.setData(uSeriesData, true)
                } else if (rafId === null) {
                    // eyeCandy, rAF paused (user zoomed): keep data live, preserve scale
                    chart!.setData(uSeriesData, false)
                }
                // if rAF is running, it calls setData each frame
                scheduleWindowStats()
            })
        }
    })
})
</script>

<template>
    <div class="p-2">
        <div id="u-plot-chart"></div>
    </div>
</template>

<style scoped lang="scss">
#u-plot-chart {
    width: 100%;
    height: var(--time-chart-height, calc(100vh - 5.75rem));
}

// zoom selection style
#u-plot-chart :deep(.u-select) {
    background: linear-gradient(
        0deg,
        rgba(var(--colors-accent) / 0.03) 0%,
        rgba(var(--colors-accent) / 0.4) 100%
    );
    position: absolute;
    pointer-events: none;
}
/** To add a crosshair to the y-axis:
.chart :deep(.u-hz .u-cursor-y) {
    border-bottom: v-bind(yCrosshair);
}
*/
</style>
