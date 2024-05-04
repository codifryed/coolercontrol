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
import * as echarts from 'echarts/core'
import { GaugeChart } from 'echarts/charts'
import VChart from 'vue-echarts'
import { type EChartsOption } from 'echarts'
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { ref, watch } from 'vue'
import { CanvasRenderer } from 'echarts/renderers'

echarts.use([GaugeChart, CanvasRenderer])

interface Props {
    deviceUID: UID
    sensorName: string
    min?: boolean
    avg?: boolean
    max?: boolean
    temp?: boolean
    duty?: boolean
    freq?: boolean
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()

// let rpm: number = 0
const sensorProperties = currentDeviceStatus.value.get(props.deviceUID)!.get(props.sensorName)!
const hasTemp: boolean = sensorProperties.temp != null
const hasDuty: boolean = sensorProperties.duty != null
const hasFreq: boolean = sensorProperties.freq != null
// const hasRPM: boolean = sensorProperties.rpm != null
const allValues: Array<number> = []

const fillAllValues = () => {
    for (const device of deviceStore.allDevices()) {
        if (device.uid != props.deviceUID) {
            continue
        }
        device.status_history
            .map((status) =>
                hasTemp
                    ? status.temps.find((temp) => temp.name === props.sensorName)?.temp ?? 0
                    : hasFreq
                      ? status.channels.find((channel) => channel.name === props.sensorName)
                            ?.freq ?? 0
                      : status.channels.find((channel) => channel.name === props.sensorName)
                            ?.duty ?? 0,
            )
            .forEach((value) => allValues.push(value))
    }
}
fillAllValues()
let min: number = 0
let max: number = 0
if (props.min) {
    min = allValues.reduce(
        (accumulator, currentValue) => Math.min(accumulator, currentValue),
        10_000,
    )
}
if (props.max || hasFreq) {
    max = allValues.reduce((accumulator, currentValue) => Math.max(accumulator, currentValue), 0)
}
const gaugeMin: number = 0
const gaugeMax: number = hasFreq ? max : 100

const getCurrentValue = (): number => {
    const currentValues = currentDeviceStatus.value.get(props.deviceUID)!.get(props.sensorName)!
    if (hasTemp) {
        return Number(currentValues.temp)
    } else if (hasFreq) {
        return Number(currentValues.freq)
    } else if (hasDuty) {
        // if (hasRPM) {
        //   rpm = Number(currentValues.rpm)
        // }
        return Number(currentValues.duty)
    } else {
        return 0
    }
}
const getDisplayValue = (): number => {
    const currentValue = getCurrentValue()
    allValues.push(currentValue)
    if (hasFreq) {
        max = Math.max(currentValue, max)
    }
    if (props.min) {
        min = Math.min(currentValue, min)
        return min
    } else if (props.avg) {
        return deviceStore.round(
            allValues.reduce((acc, currentValue) => acc + currentValue, 0) / allValues.length,
            1,
        )
    } else if (props.max) {
        max = Math.max(currentValue, max)
        return max
    } else {
        return currentValue
    }
}

const getTitle = (): string => {
    if (props.min) {
        return 'Min'
    } else if (props.avg) {
        return 'Avg'
    } else if (props.max) {
        return 'Max'
    } else if (props.temp) {
        return 'Temp'
    } else if (props.freq) {
        return 'Freq'
    } else if (props.duty) {
        return 'Duty'
    } else {
        return ''
    }
}

const getSensorColor = (): string =>
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)
        ?.sensorsAndChannels.get(props.sensorName)!.color ?? colors.themeColors.context_color

interface GaugeData {
    value: number
    name: string
}

const sensorGaugeData: Array<GaugeData> = [{ value: 0, name: getTitle() }]

const option: EChartsOption = {
    series: [
        {
            id: 'gaugeChart',
            type: 'gauge',
            min: gaugeMin,
            max: gaugeMax,
            progress: {
                show: true,
                width: deviceStore.getREMSize(0.625),
                itemStyle: {
                    color: getSensorColor(),
                },
            },
            axisLine: {
                lineStyle: {
                    width: deviceStore.getREMSize(0.625),
                    color: [[1, colors.themeColors.bg_three]],
                },
            },
            axisTick: {
                show: false,
            },
            splitLine: {
                length: 3,
                distance: 3,
                lineStyle: {
                    width: 1,
                    color: colors.themeColors.gray_600,
                },
            },
            pointer: {
                offsetCenter: [0, '15%'],
                // icon: 'triangle',
                icon: 'path://M2090.36389,615.30999 L2090.36389,615.30999 C2091.48372,615.30999 2092.40383,616.194028 2092.44859,617.312956 L2096.90698,728.755929 C2097.05155,732.369577 2094.2393,735.416212 2090.62566,735.56078 C2090.53845,735.564269 2090.45117,735.566014 2090.36389,735.566014 L2090.36389,735.566014 C2086.74736,735.566014 2083.81557,732.63423 2083.81557,729.017692 C2083.81557,728.930412 2083.81732,728.84314 2083.82081,728.755929 L2088.2792,617.312956 C2088.32396,616.194028 2089.24407,615.30999 2090.36389,615.30999 Z',
                length: '90%',
                width: deviceStore.getREMSize(0.1875),
                itemStyle: {
                    color: colors.themeColors.context_color,
                },
            },
            anchor: {
                show: true,
                size: deviceStore.getREMSize(0.375),
                itemStyle: {
                    borderWidth: 1,
                    borderColor: colors.themeColors.context_hover,
                    color: colors.themeColors.context_color,
                },
            },
            axisLabel: {
                show: false,
                distance: 12,
                color: colors.themeColors.text_color_secondary,
                fontSize: deviceStore.getREMSize(0.4),
            },
            title: {
                show: true,
                offsetCenter: [0, '-29%'],
                fontSize: deviceStore.getREMSize(0.75),
                color: colors.themeColors.text_color,
            },
            detail: {
                valueAnimation: true,
                fontSize: deviceStore.getREMSize(1),
                fontWeight: 'normal',
                color: colors.themeColors.text_color,
                offsetCenter: [0, '70%'],
                formatter: function (value) {
                    // return `${hasTemp ? value.toFixed(1) : value.toFixed(0)}${valueSuffix}`
                    return `${hasTemp ? value.toFixed(1) : value.toFixed(0)}`
                },
            },
            silent: true,
            data: sensorGaugeData,
        },
    ],
    animation: true,
    animationDurationUpdate: 300,
}

const setGaugeData = () => {
    sensorGaugeData[0].value = getDisplayValue()
}
setGaugeData()

const miniGaugeChart = ref<InstanceType<typeof VChart> | null>(null)

watch(currentDeviceStatus, () => {
    setGaugeData()
    miniGaugeChart.value?.setOption({ series: [{ id: 'gaugeChart', data: sensorGaugeData }] })
})

watch(settingsStore.allUIDeviceSettings, () => {
    const sensorColor = getSensorColor()
    // @ts-ignore
    option.series[0].progress.itemStyle.color = sensorColor
    miniGaugeChart.value?.setOption({
        series: [{ id: 'gaugeChart', progress: { itemStyle: { color: sensorColor } } }],
    })
})
</script>

<template>
    <v-chart
        class="mini-gauge-container"
        ref="miniGaugeChart"
        :option="option"
        :autoresize="true"
        :manual-update="true"
    />
</template>

<style scoped lang="scss">
.mini-gauge-container {
    height: 8rem;
    width: 100%;
}
</style>
