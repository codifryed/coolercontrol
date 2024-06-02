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
import { Device, type UID } from '@/models/Device'
import MiniGauge from '@/components/MiniGauge.vue'
import uPlot from 'uplot'
import { onMounted, ref, type Ref, watch } from 'vue'
import InputNumber from 'primevue/inputnumber'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import {
    columnHighlightPlugin,
    DeviceLineProperties,
    mouseWheelZoomPlugin,
    SCALE_KEY_PERCENT,
    tooltipPlugin,
} from '@/components/u-plot-plugins.ts'

interface Props {
    deviceId: UID
    name: string
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const uSeriesData: uPlot.AlignedData = []
const uLineName: string = props.name + '_temp'

const chartMinutesMin: number = 1
const chartMinutesMax: number = 60
const chartMinutes: Ref<number> = ref(
    settingsStore.systemOverviewOptions.selectedTimeRange.seconds / 60,
)
const chartMinutesChanged = (): void => {
    callRefreshSeriesListData()
}
const chartMinutesScrolled = (event: WheelEvent): void => {
    if (event.deltaY < 0) {
        if (chartMinutes.value < chartMinutesMax) chartMinutes.value += 1
    } else {
        if (chartMinutes.value > chartMinutesMin) chartMinutes.value -= 1
    }
}

const device: Device = [...deviceStore.allDevices()].find((dev) => dev.uid === props.deviceId)!
const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
const tempSettings = deviceSettings.sensorsAndChannels.get(props.name)!
const allDevicesLineProperties = new Map<string, DeviceLineProperties>()
allDevicesLineProperties.set(uLineName, {
    color: tempSettings.color,
    hidden: tempSettings.hide,
    name: tempSettings.name,
})

const initUSeriesData = () => {
    uSeriesData.length = 0
    const currentStatusLength = chartMinutes.value * 60
    const uTimeData = new Uint32Array(currentStatusLength)
    const uLineData = new Float32Array(currentStatusLength)
    for (const [statusIndex, status] of device.status_history
        .slice(-currentStatusLength)
        .entries()) {
        uTimeData[statusIndex] = Math.floor(new Date(status.timestamp).getTime() / 1000) // Status' Unix timestamp
        status.temps
            .filter((tempStatus) => tempStatus.name === props.name)
            .forEach((tempStatus) => (uLineData[statusIndex] = tempStatus.temp))
    }
    uSeriesData.push(uTimeData)
    uSeriesData.push(uLineData)
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
    const currentStatusLength = chartMinutes.value * 60
    shiftSeriesData(1)

    const newTimestamp = device.status.timestamp
    uSeriesData[0][currentStatusLength - 1] = Math.floor(new Date(newTimestamp).getTime() / 1000)
    device.status.temps
        .filter((tempStatus) => tempStatus.name === props.name)
        .forEach((tempStatus) => (uSeriesData[1][currentStatusLength - 1] = tempStatus.temp))
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

const lineStyle: Array<number> = []

uPlotSeries.push({
    show: true,
    label: uLineName,
    scale: '%',
    auto: false,
    stroke: tempSettings.color,
    points: {
        show: false,
    },
    dash: lineStyle,
    spanGaps: true,
    width: settingsStore.systemOverviewOptions.timeChartLineScale,
    min: 0,
    max: 100,
    value: (_, rawValue) => (rawValue != null ? rawValue.toFixed(1) : rawValue),
})

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
            incrs: [15, 60, 300],
            space: deviceStore.getREMSize(6.25),
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
            label: 'temperature °C',
            labelGap: 3,
            labelSize: deviceStore.getREMSize(1.3),
            labelFont: `bold ${deviceStore.getREMSize(1.0)}px sans-serif`,
            stroke: colors.themeColors.text_color,
            size: deviceStore.getREMSize(2.0),
            font: `${deviceStore.getREMSize(1)}px sans-serif`,
            gap: 0,
            ticks: {
                show: true,
                stroke: colors.themeColors.text_color,
                width: 1,
                size: 5,
            },
            incrs: [10],
            // values: (_, ticks) => ticks.map((rawValue) => rawValue + '° '),
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
    ],
    scales: {
        '%': {
            auto: false,
            range: [0, 100],
        },
        x: {
            auto: true,
            time: true,
        },
    },
    legend: {
        show: false,
    },
    cursor: {
        show: true,
        x: false,
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

onMounted(async () => {
    const uChartElement: HTMLElement = document.getElementById('u-plot-chart') ?? new HTMLElement()
    const uPlotChart = new uPlot(uOptions, uSeriesData, uChartElement)
    let isZoomed: boolean = false

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

    watch(settingsStore.systemOverviewOptions, () => {
        callRefreshSeriesListData()
        uPlotSeries[1].width = settingsStore.systemOverviewOptions.timeChartLineScale
        uPlotChart.delSeries(1)
        uPlotChart.addSeries(uPlotSeries[1], 1)
        uPlotChart.redraw()
    })

    watch(settingsStore.allUIDeviceSettings, () => {
        uPlotSeries[1].stroke = tempSettings.color
        uPlotChart.delSeries(1)
        uPlotChart.addSeries(uPlotSeries[1], 1)
        uPlotChart.redraw()
    })

    // @ts-ignore
    document?.querySelector('.chart-minutes')?.addEventListener('wheel', chartMinutesScrolled)
    watch(chartMinutes, chartMinutesChanged)
})
</script>

<template>
    <div class="card pt-2">
        <div class="flex">
            <div class="flex-inline control-column">
                <InputNumber
                    placeholder="Minutes"
                    input-id="chart-minutes"
                    v-model="chartMinutes"
                    mode="decimal"
                    class="chart-minutes w-full mb-6 mt-2"
                    suffix=" min"
                    show-buttons
                    :step="1"
                    :min="chartMinutesMin"
                    :max="chartMinutesMax"
                    :input-style="{ width: '60px' }"
                    :allow-empty="false"
                />
                <MiniGauge :device-u-i-d="props.deviceId" :sensor-name="props.name" min />
                <MiniGauge :device-u-i-d="props.deviceId" :sensor-name="props.name" avg />
                <MiniGauge :device-u-i-d="props.deviceId" :sensor-name="props.name" max />
            </div>
            <div class="flex-1 p-0 pt-0">
                <div id="u-plot-chart" class="chart"></div>
            </div>
        </div>
    </div>
</template>

<style scoped lang="scss">
.control-column {
    width: 14rem;
    padding-right: 1rem;
}

.chart {
    width: 99%;
    height: calc(100vh - 7.95rem);
}
</style>
