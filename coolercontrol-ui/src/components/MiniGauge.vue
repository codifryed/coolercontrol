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
import {GaugeChart} from 'echarts/charts'
import VChart from 'vue-echarts'
import {type EChartsOption} from "echarts"
import {type UID} from "@/models/Device"
import {useDeviceStore} from "@/stores/DeviceStore"
import {storeToRefs} from "pinia"
import {useSettingsStore} from "@/stores/SettingsStore"
import {useThemeColorsStore} from "@/stores/ThemeColorsStore"
import {onMounted, ref, watch} from "vue"
import {CanvasRenderer} from "echarts/renderers";

echarts.use([
  GaugeChart, CanvasRenderer
])

interface Props {
  deviceUID: UID,
  sensorName: string,
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const {currentDeviceStatus} = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()

const gaugeMin: number = 0
const gaugeMax: number = 100

let rpm: number = 0
const sensorProperties = currentDeviceStatus.value.get(props.deviceUID)!.get(props.sensorName)!
const hasTemp: boolean = sensorProperties.temp != null
const hasDuty: boolean = sensorProperties.duty != null
const hasRPM: boolean = sensorProperties.rpm != null

const getSensorValue = (): number => {
  const values = currentDeviceStatus.value.get(props.deviceUID)!.get(props.sensorName)!
  if (hasTemp) {
    return Number(values.temp)
  } else if (hasDuty) {
    if (hasRPM) {
      rpm = Number(values.rpm)
    }
    return Number(values.duty)
  } else {
    return 0
  }
}

const valueSuffix: string = hasTemp ? '°' : hasDuty ? '%' : ''

const initOptions = {
  useDirtyRect: false, // true causes some issues with animations and opaque lines
  renderer: 'canvas',
}

const getSensorColor = (): string => settingsStore.allDeviceSettings
    .get(props.deviceUID)?.sensorsAndChannels
    .getValue(props.sensorName)
    .color ?? colors.themeColors().context_color;

interface GaugeData {
  value: number
}

const sensorGaugeData: Array<GaugeData> = [{value: 0}]

const option: EChartsOption = {
  series: [
    {
      id: 'gaugeChart',
      type: 'gauge',
      min: gaugeMin,
      max: gaugeMax,
      progress: {
        show: true,
        width: 10,
        itemStyle: {
          color: getSensorColor(),
        },
      },
      axisLine: {
        lineStyle: {
          width: 10,
          color: [[1, colors.themeColors().bg_three]],
        }
      },
      axisTick: {
        show: false
      },
      splitLine: {
        length: 3,
        distance: 3,
        lineStyle: {
          width: 1,
          color: colors.themeColors().text_description
        }
      },
      pointer: {
        offsetCenter: [0, '15%'],
        // icon: 'triangle',
        icon: 'path://M2090.36389,615.30999 L2090.36389,615.30999 C2091.48372,615.30999 2092.40383,616.194028 2092.44859,617.312956 L2096.90698,728.755929 C2097.05155,732.369577 2094.2393,735.416212 2090.62566,735.56078 C2090.53845,735.564269 2090.45117,735.566014 2090.36389,735.566014 L2090.36389,735.566014 C2086.74736,735.566014 2083.81557,732.63423 2083.81557,729.017692 C2083.81557,728.930412 2083.81732,728.84314 2083.82081,728.755929 L2088.2792,617.312956 C2088.32396,616.194028 2089.24407,615.30999 2090.36389,615.30999 Z',
        length: '90%',
        width: 3,
        itemStyle: {
          color: colors.themeColors().context_color,
        }
      },
      anchor: {
        show: true,
        size: 6,
        itemStyle: {
          borderWidth: 1,
          borderColor: colors.themeColors().context_hover,
          color: colors.themeColors().context_color,
        }
      },
      axisLabel: {
        distance: 12,
        color: colors.themeColors().text_description,
        fontSize: 8
      },
      title: {
        show: false
      },
      detail: {
        valueAnimation: true,
        fontSize: 20,
        color: colors.themeColors().text_title,
        offsetCenter: [0, '70%'],
        formatter: function (value) {
          return `${hasTemp ? value.toFixed(1) : value}${valueSuffix}`
        }
      },
      silent: true,
      data: sensorGaugeData,
    },
  ],
  animation: true,
  animationDurationUpdate: 300,
}

const setGaugeData = () => {
  sensorGaugeData[0].value = getSensorValue()
}
setGaugeData()

const miniGaugeChart = ref<InstanceType<typeof VChart> | null>(null)

watch(currentDeviceStatus, () => {
  setGaugeData()
  miniGaugeChart.value?.setOption({series: [{id: 'gaugeChart', data: sensorGaugeData}]})
})

watch(settingsStore.allDeviceSettings, () => {
  const sensorColor = getSensorColor()
  // @ts-ignore
  option.series[0].progress.itemStyle.color = sensorColor
  miniGaugeChart.value?.setOption({series: [{id: 'gaugeChart', progress: {itemStyle: {color: sensorColor}}}]})
})
</script>

<template>
  <v-chart class="mini-gauge-container" ref="miniGaugeChart" :init-options="initOptions" :option="option"
           :autoresize="true" :manual-update="true"/>
</template>

<style scoped lang="scss">
.mini-gauge-container {
  height: 192px;
  width: 100%;
}
</style>