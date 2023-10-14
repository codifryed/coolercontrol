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
import * as echarts from 'echarts/core'
import {GraphicComponent, GridComponent, MarkAreaComponent, TooltipComponent,} from 'echarts/components'
import {LineChart} from 'echarts/charts'
import {UniversalTransition} from 'echarts/features'
import {CanvasRenderer} from 'echarts/renderers'
import VChart from 'vue-echarts'
import {type EChartsOption} from "echarts"
import {Profile} from "@/models/Profile"
import {type UID} from "@/models/Device"
import {useDeviceStore} from "@/stores/DeviceStore"
import {storeToRefs} from "pinia"
import {useSettingsStore} from "@/stores/SettingsStore"
import {useThemeColorsStore} from "@/stores/ThemeColorsStore"
import {onMounted, ref, watch} from "vue"

echarts.use([
  GridComponent, LineChart, CanvasRenderer, UniversalTransition, TooltipComponent, GraphicComponent, MarkAreaComponent
])

interface Props {
  profile: Profile
  currentDeviceUID: UID
  currentSensorName: string
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const {currentDeviceStatus} = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()

const axisXTempMin: number = 0
const axisXTempMax: number = 100
const dutyMin: number = 0
const dutyMax: number = 100

interface LineData {
  value: number[]
}

const deviceDutyLineData: [LineData, LineData] = [{value: []}, {value: []}]
const getFixedDuty = (): number => {
  return props.profile.speed_duty ?? 0
}
const fixedDutyLineData: [LineData, LineData] = [
  {value: [axisXTempMin, getFixedDuty()]},
  {value: [axisXTempMax, getFixedDuty()]}
]

const getDeviceDutyLineColor = (): string => {
  return settingsStore.allDeviceSettings.get(props.currentDeviceUID)?.sensorsAndChannels
      .getValue(props.currentSensorName)
      .color ?? colors.themeColors().yellow
}

const initOptions = {
  useDirtyRect: true,
  renderer: 'canvas',
}

const option: EChartsOption = {
  grid: {
    show: false,
    top: 10,
    left: 10,
    right: 15,
    bottom: 0,
    containLabel: true,
  },
  xAxis: {
    min: axisXTempMin,
    max: axisXTempMax,
    type: 'value',
    splitNumber: 10,
    axisLabel: {
      formatter: '{value}°'
    },
    axisLine: {
      lineStyle: {
        color: colors.themeColors().text_active,
        width: 1,
      }
    },
    splitLine: {
      lineStyle: {
        color: colors.themeColors().text_description,
        type: 'dotted'
      }
    },
  },
  yAxis: {
    min: dutyMin,
    max: dutyMax,
    type: 'value',
    splitNumber: 10,
    axisLabel: {
      formatter: '{value}%'
    },
    axisLine: {
      lineStyle: {
        color: colors.themeColors().text_active,
        width: 1,
      }
    },
    splitLine: {
      lineStyle: {
        color: colors.themeColors().text_description,
        type: 'dotted'
      }
    },
  },
  // @ts-ignore
  series: [
    {
      id: 'dutyLine',
      type: 'line',
      smooth: false,
      symbol: 'none',
      lineStyle: {
        color: getDeviceDutyLineColor(),
        width: 2,
        type: 'solid',
      },
      emphasis: {
        disabled: true,
      },
      data: deviceDutyLineData,
      z: 100,
      silent: true,
    },
    {
      id: 'fixedDutyLine',
      type: 'line',
      smooth: false,
      symbol: 'none',
      lineStyle: {
        color: {
          type: 'linear',
          x: 0,
          y: 0,
          x2: 0,
          y2: 1,
          colorStops: [{
            offset: 0, color: `${colors.themeColors().green}00`
          }, {
            offset: 0.2, color: `${colors.themeColors().green}18`
          }, {
            offset: 0.45, color: `${colors.themeColors().green}70`
          }, {
            offset: 0.5, color: `${colors.themeColors().green}90`
          }, {
            offset: 0.45, color: `${colors.themeColors().green}70`
          }, {
            offset: 0.8, color: `${colors.themeColors().green}18`
          }, {
            offset: 1, color: `${colors.themeColors().green}00`
          }],
        },
        width: 12,
      },
      emphasis: {
        disabled: true,
      },
      data: fixedDutyLineData,
      z: 1,
      silent: true,
    },
  ],
  animation: true,
  animationDurationUpdate: 800,
}

const getDuty = (): number => {
  return Number(currentDeviceStatus.value.get(props.currentDeviceUID)?.get(props.currentSensorName)?.duty) ?? 0
}

const setGraphData = () => {
  const duty = getDuty()
  deviceDutyLineData[0].value = [axisXTempMin, duty]
  deviceDutyLineData[1].value = [axisXTempMax, duty]
}
setGraphData()

const controlGraph = ref<InstanceType<typeof VChart> | null>(null)

watch(currentDeviceStatus, () => {
  const duty = getDuty()
  if (duty === 0) {
    return
  }
  deviceDutyLineData[0].value = [axisXTempMin, duty]
  deviceDutyLineData[1].value = [axisXTempMax, duty]
  controlGraph.value?.setOption({series: {id: 'dutyLine', data: deviceDutyLineData}})
})

watch(settingsStore.allDeviceSettings, () => {
  const lineColor = getDeviceDutyLineColor()
  // @ts-ignore
  option.series[0].lineStyle.color = lineColor
  controlGraph.value?.setOption({series: {id: 'dutyLine', lineStyle: {color: lineColor}}})
})
</script>

<template>
  <v-chart class="control-graph" ref="controlGraph" :init-options="initOptions" :option="option"
           :autoresize="true" :manual-update="true"/>
</template>

<style scoped lang="scss">
.control-graph {
  height: 80vh;
  width: 99.9%; // This handles an issue with the graph when the layout thinks it's too big for the container
}
</style>