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

import {Device, type UID} from "@/models/Device";
import MiniGauge from "@/components/MiniGauge.vue";
import uPlot from "uplot";
import {onMounted, onUnmounted, ref, type Ref, watch} from "vue";
import Dropdown from "primevue/dropdown";
import {useDeviceStore} from "@/stores/DeviceStore";
import {useSettingsStore} from "@/stores/SettingsStore";
import {useThemeColorsStore} from "@/stores/ThemeColorsStore";

interface Props {
  deviceId: UID
  name: string
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const uSeriesData: uPlot.AlignedData = []
const uLineName: string = props.name

const timeRanges: Ref<Array<{ name: string; seconds: number; }>> = ref([
  {name: '1 min', seconds: 60},
  {name: '5 min', seconds: 300},
  {name: '15 min', seconds: 900},
  {name: '30 min', seconds: 1800},
])
const selectedTimeRange = ref(settingsStore.systemOverviewOptions.selectedTimeRange)

const device: Device = [...deviceStore.allDevices()].find((dev) => dev.uid === props.deviceId)!
const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
const tempSettings = deviceSettings.sensorsAndChannels.getValue(props.name)

const initUSeriesData = () => {
  uSeriesData.length = 0
  const currentStatusLength = Math.min(selectedTimeRange.value.seconds, device.status_history.length)
  const uTimeData = new Uint32Array(currentStatusLength)
  const uLineData = new Float32Array(currentStatusLength)
  for (const [statusIndex, status] of device.status_history.slice(-currentStatusLength).entries()) {
    uTimeData[statusIndex] = Math.floor(new Date(status.timestamp).getTime() / 1000) // Status' Unix timestamp
    status.channels
        .filter((channelStatus) => channelStatus.name === props.name)
        .forEach((channelStatus) => uLineData[statusIndex] = channelStatus.duty ?? 0)
  }
  uSeriesData.push(uTimeData)
  uSeriesData.push(uLineData)
  console.debug("Initialized uPlot Series Data")
}


const shiftSeriesData = (shiftLength: number) => {
  for (const arr of uSeriesData) {
    for (let i = 0; i < arr.length - shiftLength; i++) {
      arr[i] = arr[i + shiftLength] // Shift left
    }
  }
}


const updateUSeriesData = () => {
  const currentStatusLength = Math.min(selectedTimeRange.value.seconds, device.status_history.length)
  const growStatus = uSeriesData[0].length < currentStatusLength // happens when the status history has just started being populated
  if (growStatus) {
    // create new larger Arrays - typed arrays are a fixed size - and fill in the old data
    const uTimeData = new Uint32Array(currentStatusLength)
    uTimeData.set(uSeriesData[0])
    uSeriesData[0] = uTimeData
    const uLineData = new Float32Array(currentStatusLength)
    uLineData.set(uSeriesData[1])
    uSeriesData[1] = uLineData
  } else {
    shiftSeriesData(1)
  }

  const newTimestamp = device.status_history.slice(-1)[0].timestamp
  uSeriesData[0][currentStatusLength - 1] = Math.floor(new Date(newTimestamp).getTime() / 1000)
  const newStatus = device.status_history.slice(-1)[0]
  newStatus.channels
      .filter((channelStatus) => channelStatus.name === props.name)
      .forEach((channelStatus) => uSeriesData[1][currentStatusLength - 1] = channelStatus.duty ?? 0)
  console.debug("Updated uPlot Data")
}

let refreshSeriesListData = () => {
  initUSeriesData()
}

initUSeriesData()

const uPlotSeries: Array<uPlot.Series> = [
  {}
]

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
  width: 1.6,
  min: 0,
  max: 100,
  value: (_, rawValue) => rawValue != null ? rawValue.toFixed(0) : rawValue,
})

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
      size: deviceStore.getREMSize(1.5),
      font: `${deviceStore.getREMSize(1)}px rounded`,
      ticks: {
        show: true,
        stroke: colors.themeColors().text_title,
        width: 1,
      },
      incrs: [15, 60, 300],
      space: 100,
      values: [
        // min tick incr | default | year | month | day | hour | min | sec | mode
        [300, "{h}:{mm}", null, null, null, null, null, null, 0],
        [60, "{h}:{mm}", null, null, null, null, null, null, 0],
        [15, "{h}:{mm}:{ss}", null, null, null, null, null, null, 0],
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
      size: deviceStore.getREMSize(1.5),
      font: `${deviceStore.getREMSize(1)}px rounded`,
      ticks: {
        show: true,
        stroke: colors.themeColors().text_title,
        width: 1,
      },
      incrs: [10],
      values: (_, ticks) => ticks.map(rawValue => rawValue + "%"),
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
  ],
  scales: {
    "%": {
      auto: false,
      range: [0, 100],
    },
    x: {
      auto: true,
      time: true,
    }
  },
  legend: {
    show: false,
  },
  cursor: {
    show: false,
  }
}

onMounted(async () => {
  const uChartElement: HTMLElement = document.getElementById('u-plot-chart') ?? new HTMLElement()
  const uPlotChart = new uPlot(uOptions, uSeriesData, uChartElement)

  const getChartSize = () => {
    const cwh = uChartElement.getBoundingClientRect()
    return {width: cwh.width, height: cwh.height}
  }
  uPlotChart.setSize(getChartSize())
  const resizeObserver = new ResizeObserver((_) => {
    uPlotChart.setSize(getChartSize());
  })
  resizeObserver.observe(uChartElement)

  refreshSeriesListData = () => {
    initUSeriesData()
    uPlotChart.setData(uSeriesData)
  }

  deviceStore.$onAction(({name, after}) => {
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
        console.warn("Complete Status History loaded")
        initUSeriesData()
        uPlotChart.setData(uSeriesData)
      })
    }
  })
  watch(settingsStore.allUIDeviceSettings, () => {
    uPlotSeries[1].stroke = tempSettings.color
    uPlotChart.delSeries(1)
    uPlotChart.addSeries(uPlotSeries[1], 1)
    uPlotChart.redraw()
  })
})
</script>

<template>
  <div class="card pt-2">
    <div class="grid">
      <div class="col-fixed" style="width: 10rem">
        <Dropdown v-model="selectedTimeRange" :options="timeRanges"
                  placeholder="Select a Time Range"
                  option-label="name" class="w-full mb-6 mt-2" v-on:change="refreshSeriesListData"/>
        <MiniGauge :device-u-i-d="props.deviceId" :sensor-name="props.name" min/>
        <MiniGauge :device-u-i-d="props.deviceId" :sensor-name="props.name" avg/>
        <MiniGauge :device-u-i-d="props.deviceId" :sensor-name="props.name" max/>
      </div>
      <div class="col">
        <div id="u-plot-chart" class="chart"></div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.chart {
  width: 100%;
  height: calc(100vh - 9rem);
}
</style>