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
import {useDeviceStore} from "@/stores/DeviceStore"
import {useSettingsStore} from "@/stores/SettingsStore"
import {onMounted, type Ref, ref, watch} from "vue"
import {type Color, Device} from "@/models/Device"
import {DefaultDictionary} from "typescript-collections"
import Dropdown from 'primevue/dropdown'
import uPlot from 'uplot'
import 'uplot/dist/uPlot.min.css'

const selectedChartType = ref('TimeChart')
const chartTypes = ref([
  'TimeChart',
  'Current',
  'Table',
])

const selectedTimeRange: Ref<{ name: string; seconds: number; }> = ref({name: '1 min', seconds: 60})
const timeRanges: Ref<Array<{ name: string; seconds: number; }>> = ref([
  {name: '1 min', seconds: 60},
  {name: '5 min', seconds: 300},
  {name: '15 min', seconds: 900},
  {name: '30 min', seconds: 1800},
])

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const uSeriesData: uPlot.AlignedData = []
const uLineNames: Array<string> = []
const allDevicesLineColors = new Map<string, Color>()

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
  // this is needed for when the daemon first start up:
  const currentStatusLength = Math.min(selectedTimeRange.value.seconds, firstDevice.status_history.length)
  const uTimeData = new Uint32Array(currentStatusLength)
  for (const [statusIndex, status] of firstDevice.status_history.slice(-currentStatusLength).entries()) {
    uTimeData[statusIndex] = Math.floor(new Date(status.timestamp).getTime() / 1000) // Status' Unix timestamp
  }

  // line values should not be greater than 100 and not less than 0 so Uint8 should be fine.
  // TypedArrays have a fixed length, so we need to manage this ourselves
  const uLineData = new DefaultDictionary<string, Uint8Array>(() => new Uint8Array(currentStatusLength))

  for (const device of deviceStore.allDevices()) {
    const deviceSettings = settingsStore.allDeviceSettings.get(device.uid)!
    for (const [statusIndex, status] of device.status_history.slice(-currentStatusLength).entries()) {
      for (const tempStatus of status.temps) {
        const tempSettings = deviceSettings.sensorsAndChannels.getValue(tempStatus.name)
        const lineName = createLineName(device, tempStatus.name)
        if (!uLineNames.includes(lineName)) {
          uLineNames.push(lineName)
        }
        if (!allDevicesLineColors.has(lineName)) {
          allDevicesLineColors.set(lineName, tempSettings.color)
        }
        uLineData.getValue(lineName)[statusIndex] = tempStatus.temp
      }
      for (const channelStatus of status.channels) {
        if (channelStatus.duty != null) { // check for null or undefined
          const channelSettings = deviceSettings.sensorsAndChannels.getValue(channelStatus.name)
          const lineName = createLineName(device, channelStatus.name)
          if (!uLineNames.includes(lineName)) {
            uLineNames.push(lineName)
          }
          if (!allDevicesLineColors.has(lineName)) {
            allDevicesLineColors.set(lineName, channelSettings.color)
          }
          uLineData.getValue(lineName)[statusIndex] = channelStatus.duty
        }
      }
    }
  }

  for (const lineName of uLineNames) { // the uLineNames Array keeps our LineData arrays in order
    uSeriesData.push(uLineData.getValue(lineName))
  }
  uSeriesData.splice(0, 0, uTimeData) // 'inserts' time values as the first array, where uPlot expects it
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
  const firstDevice: Device = deviceStore.allDevices().next().value
  // this is needed for when the daemon first start up:
  const currentStatusLength = Math.min(selectedTimeRange.value.seconds, firstDevice.status_history.length)
  const growStatus = uSeriesData[0].length < currentStatusLength // happens when the status history has just started being populated
  if (growStatus) {
    // create new larger Arrays - typed arrays are a fixed size - and fill in the old data
    const uTimeData = new Uint32Array(currentStatusLength)
    uTimeData.set(uSeriesData[0])
    uSeriesData[0] = uTimeData
    const uLineData = new DefaultDictionary<string, Uint8Array>(() => new Uint8Array(currentStatusLength))
    for (const [lineIndex, lineName] of uLineNames.entries()) {
      uLineData.getValue(lineName).set(uSeriesData[lineIndex + 1])
      uSeriesData[lineIndex + 1] = uLineData.getValue(lineName)
    }
  } else {
    shiftSeriesData(1)
  }

  const newTimestamp = firstDevice.status_history.slice(-1)[0].timestamp
  uSeriesData[0][currentStatusLength - 1] = Math.floor(new Date(newTimestamp).getTime() / 1000)

  for (const device of deviceStore.allDevices()) {
    const newStatus = device.status_history.slice(-1)[0]
    for (const tempStatus of newStatus.temps) {
      const lineName = createLineName(device, tempStatus.name)
      uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] = tempStatus.temp
    }
    for (const channelStatus of newStatus.channels) {
      if (channelStatus.duty != null) { // check for null or undefined
        const lineName = createLineName(device, channelStatus.name)
        uSeriesData[uLineNames.indexOf(lineName) + 1][currentStatusLength - 1] = channelStatus.duty
      }
    }
  }
  console.debug("Updated uPlot Data")
}

let refreshSeriesListData = () => {
  initUSeriesData()
}

initUSeriesData()

const uPlotSeries: Array<uPlot.Series> = [
  {}
]

const getLineStyle = (lineName: string): Array<number> => {
  const lineLower = lineName.toLowerCase()
  if (lineLower.includes("fan")) {
    return [10, 3, 2, 3]
  } else if (lineLower.includes("load") || lineLower.includes("pump")) {
    return [6, 3]
  } else {
    return []
  }
}
for (const lineName of uLineNames) {
  uPlotSeries.push({
    label: lineName,
    scale: '%',
    auto: false,
    stroke: allDevicesLineColors.get(lineName),
    points: {
      show: false,
    },
    dash: getLineStyle(lineName),
    spanGaps: true,
    width: 1.6,
    min: 0,
    max: 100,
    value: (_, rawValue) => rawValue != null ? rawValue.toFixed(1) : rawValue,
  })
}

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
      stroke: '#ccc',
      ticks: {
        show: true,
        stroke: '#ccc',
        width: 1,
      },
      incrs: [15, 60, 300],
      // values: [
      //     [15, ":{ss}", null, null, null, "{h}:{mm}:{ss}", "{mm}:{ss}", null, 0],
      //     [300, "{mm}:{ss}", null, null, null, "{h}:{mm}:{ss}", "{mm}:{ss}", null, 0],
      // ],
      border: {
        show: true,
        width: 1,
        stroke: '#ccc',
      },
      grid: {
        show: true,
        stroke: '#4f5b6e',
        width: 1,
        dash: [1, 3],
      },
    },
    {
      scale: '%',
      label: '',
      // gap: 5, // gap for tick text from edge of graph
      stroke: '#ccc',
      ticks: {
        show: true,
        stroke: '#ccc',
        width: 1,
        // size: 10,
      },
      incrs: [10],
      values: (_, ticks) => ticks.map(rawValue => rawValue + "°/%"),
      border: {
        show: true,
        width: 1,
        stroke: '#ccc',
      },
      grid: {
        show: true,
        stroke: '#4f5b6e',
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
      // range: (min, max) => [uSeriesData[0].splice(-61)[0], uSeriesData[0].splice(-1)[0]],
      // range: timeRange(),
      // range: (min, max) => [((Date.now() / 1000) - 60), uPlotSeries[]],
    }
  },
  legend: {
    show: false,
  },
  cursor: {
    show: false,
    // focus: {
    //   prox: 10,
    // }
  }
}
console.debug('Processed status data for System Overview')

//----------------------------------------------------------------------------------------------------------------------

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

  watch(settingsStore, () => {
    // re-set all line colors on device settings change
    for (const device of deviceStore.allDevices()) {
      const deviceSettings = settingsStore.allDeviceSettings.get(device.uid)!
      for (const tempStatus of device.status.temps) {
        allDevicesLineColors.set(
            createLineName(device, tempStatus.name),
            deviceSettings.sensorsAndChannels.getValue(tempStatus.name).color
        )
      }
      for (const channelStatus of device.status.channels) {
        if (channelStatus.duty != null) { // check for null or undefined
          allDevicesLineColors.set(
              createLineName(device, channelStatus.name),
              deviceSettings.sensorsAndChannels.getValue(channelStatus.name).color
          )
        }
      }
    }
    for (const [index, lineName] of uLineNames.entries()) {
      const seriesIndex = index + 1
      uPlotSeries[seriesIndex].stroke = allDevicesLineColors.get(lineName)
      uPlotChart.delSeries(seriesIndex)
      uPlotChart.addSeries(uPlotSeries[seriesIndex], seriesIndex)
    }
    uPlotChart.redraw()
  })

})
</script>

<template>
  <main>
    <div class="card">
      <div class="flex justify-content-end flex-wrap card-container">
        <Dropdown disabled v-model="selectedChartType" :options="chartTypes" placeholder="Select a Chart Type"
                  class="w-full md:w-10rem"/>
        <Dropdown v-model="selectedTimeRange" :options="timeRanges" placeholder="Select a Time Range"
                  option-label="name" class="w-full md:w-10rem" v-on:change="refreshSeriesListData"/>
      </div>
      <div id="u-plot-chart" class="chart"></div>
    </div>
  </main>
</template>

<style scoped>
.chart {
  width: 100%;
  height: 80vh;
}
</style>