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
import {useDeviceStore} from "../stores/DeviceStore"
import {onMounted, type Ref, ref} from "vue"
import {Color, Device} from "../models/Device"
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

// const maxStatusLength: number = 2000 // can be adjusted if we want to store more than 30 mins of data now
const deviceStore = useDeviceStore()

let cpuCount: number = 0
let gpuCount: number = 0

/**
 * TimeData requires a 'name' property if we want the animation to be smooth and not 'wiggle'
 */
type TimeData = {
  name: string,
  value: [Date, number],
}


const uSeriesData: uPlot.AlignedData = []
// several arrays & dicts -> larger tranformation of data
const uLineNames: Array<string> = []
const uLineSeriesDict: DefaultDictionary<string, Array<{ time: number, value: number }>> = new DefaultDictionary(() => [])
const uTimeLineSeriesDict: DefaultDictionary<number, DefaultDictionary<string, number | null>> = new DefaultDictionary(
    () => new DefaultDictionary(() => null)
)
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
  uLineSeriesDict.clear()
  uTimeLineSeriesDict.clear()

  for (const device of deviceStore.allDevices()) {
    for (const status of device.status_history.slice(-selectedTimeRange.value.seconds)) { // get the selected time range of recent statuses
      const statusUnixEpoch = Math.floor(new Date(status.timestamp).getTime() / 1000)
      for (const tempStatus of status.temps) {
        const lineName = createLineName(device, tempStatus.name)
        uLineSeriesDict.getValue(lineName)
            .push({time: statusUnixEpoch, value: tempStatus.temp})
        allDevicesLineColors.set(lineName, device.colors.getValue(tempStatus.name))
      }
      for (const channelStatus of status.channels) {
        if (channelStatus.duty != null) { // check for null or undefined
          const lineName = createLineName(device, channelStatus.name)
          uLineSeriesDict.getValue(lineName)
              .push({time: statusUnixEpoch, value: channelStatus.duty})
          allDevicesLineColors.set(lineName, device.colors.getValue(channelStatus.name))
        }
      }
    }
  }

  const timeValues = new Uint32Array(selectedTimeRange.value.seconds)

  uLineSeriesDict.forEach((lineName, lineData) => {
    uLineNames.push(lineName)
    for (const [lineDataIndex, {time, value}] of lineData.entries()) {
      timeValues[lineDataIndex] = time // todo: this only needs to be done once really (speedup? probably not)
      uTimeLineSeriesDict.getValue(time).setValue(lineName, value)
    }
  })

  if (timeValues.length > selectedTimeRange.value.seconds) {
    console.error("There appears to be some kind of cross-time-boundry issue.")
  }

  for (const _ of uLineNames) { // add an array for each line - (xTimeArray is inserted at the end of this logic)
    // line values should not be greater than 100 and not less than 0 so Uint8 should be fine.
    // TypedArrays have a fixed length, so we need to manage this ourselves
    uSeriesData.push(new Uint8Array(selectedTimeRange.value.seconds))
  }

  for (const [timeIndex, time] of timeValues.entries()) {
    const lineValueDict = uTimeLineSeriesDict.getValue(time)
    // used in case we have an issue where timestamps are not aligned:
    const previousLineValues = new DefaultDictionary<string, number>(() => 0)
    for (const [lineIndex, lineName] of uLineNames.entries()) {
      const currentSensorValue: number | null = lineValueDict.getValue(lineName)
      if (currentSensorValue != null) {
        previousLineValues.setValue(lineName, currentSensorValue)
        uSeriesData[lineIndex][timeIndex] = currentSensorValue
      } else {
        uSeriesData[lineIndex][timeIndex] = previousLineValues.getValue(lineName)
      }
    }
  }

  uSeriesData.splice(0, 0, timeValues)
  console.debug("Initialized uPlot Series Data")
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
        value: (self, rawValue) => rawValue != null ? rawValue.toFixed(1) : rawValue,
      }
  )
}

const shiftSeriesData = (shiftLength: number) => {
  for (const arr of uSeriesData) {
    for (let i = 0; i < arr.length - 1; i++) {
      arr[i] = arr[i + 1] // Shift left
    }
  }
}

const updateUSeriesData = () => {
  const updateSize: number = 1
  for (const device of deviceStore.allDevices()) {
    for (const status of device.status_history.slice(-updateSize)) { // get most recent status
      const statusUnixEpoch = Math.floor(new Date(status.timestamp).getTime() / 1000)
      for (const tempStatus of status.temps) {
        uLineSeriesDict.getValue(createLineName(device, tempStatus.name)).shift()
        uLineSeriesDict.getValue(createLineName(device, tempStatus.name))
            .push({time: statusUnixEpoch, value: tempStatus.temp})
      }
      for (const channelStatus of status.channels) {
        if (channelStatus.duty != null) { // check for null or undefined
          uLineSeriesDict.getValue(createLineName(device, channelStatus.name)).shift()
          uLineSeriesDict.getValue(createLineName(device, channelStatus.name))
              .push({time: statusUnixEpoch, value: channelStatus.duty})
        }
      }
    }
  }

  const timeValues = new Uint32Array(updateSize)

  for (let i = 0; i < updateSize; i++) {
    uTimeLineSeriesDict.remove(uSeriesData[0][i])
  }

  uLineSeriesDict.forEach((lineName, lineData) => {
    for (const [lineDataIndex, {time, value}] of lineData.slice(-updateSize).entries()) {
      // const seriesPosition = uSeriesData[0].length - updateSize + lineDataIndex
      timeValues[lineDataIndex] = time
      uTimeLineSeriesDict.getValue(time).setValue(lineName, value)
    }
  })

  shiftSeriesData(updateSize)

  for (const [timeIndex, time] of timeValues.entries()) {
    const seriesPosition = uSeriesData[0].length - updateSize + timeIndex
    uSeriesData[0][seriesPosition] = time
    const lineValueDict = uTimeLineSeriesDict.getValue(time)
    for (const [index, lineName] of uLineNames.entries()) {
      uSeriesData[index + 1][seriesPosition] =
          // backup for not-synced time values:
          lineValueDict.getValue(lineName) ?? uSeriesData[index + 1][Math.max(seriesPosition - 1, 0)]
    }
  }

  console.debug("Updated uPlot Data")
}

let refreshSeriesListData = () => {
  initUSeriesData()
}

const uOptions: uPlot.Options = {
  width: 200,
  height: 200,
  select: { // todo: use appropriate left, right, top, bottom
    show: false,
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
      values: (self, ticks) => ticks.map(rawValue => rawValue + "°/%"),
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
        initUSeriesData()
        uPlotChart.setData(uSeriesData)
      })
      console.warn("Complete Status History loaded")
    }
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