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
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import { type EChartsOption } from 'echarts'
import { Profile } from '@/models/Profile'
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { ref, watch } from 'vue'

echarts.use([GaugeChart, CanvasRenderer])

interface Props {
    profile: Profile
    currentDeviceUID: UID
    currentSensorName: string
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()

const dutyMin: number = 0
const dutyMax: number = 100

interface GaugeData {
    value: number
}

const dutyGaugeData: Array<GaugeData> = [{ value: 0 }]
const rpmGaugeData: Array<GaugeData> = [{ value: 0 }]

const getDutySensorColor = (): string => {
    return (
        settingsStore.allUIDeviceSettings
            .get(props.currentDeviceUID)
            ?.sensorsAndChannels.get(props.currentSensorName)!.color ??
        colors.themeColors.context_color
    )
}

const getDuty = (): number => {
    return Number(
        currentDeviceStatus.value.get(props.currentDeviceUID)?.get(props.currentSensorName)?.duty ??
            -1,
    )
}

const getRPMs = (): number => {
    return (
        Number(
            currentDeviceStatus.value.get(props.currentDeviceUID)?.get(props.currentSensorName)
                ?.rpm,
        ) ?? -1
    )
}

const option: EChartsOption = {
    series: [
        {
            id: 'gaugeChart',
            type: 'gauge',
            min: dutyMin,
            max: dutyMax,
            progress: {
                show: true,
                width: deviceStore.getREMSize(2.5),
                itemStyle: {
                    color: getDutySensorColor(),
                },
            },
            axisLine: {
                lineStyle: {
                    width: deviceStore.getREMSize(2.5),
                    color: [[1, colors.themeColors.bg_three]],
                },
            },
            axisTick: {
                show: true,
                distance: -deviceStore.getREMSize(2.75),
                length: deviceStore.getREMSize(0.25),
                lineStyle: {
                    color: colors.themeColors.text_color_secondary,
                },
            },
            splitLine: {
                length: deviceStore.getREMSize(0.5),
                distance: -deviceStore.getREMSize(3),
                lineStyle: {
                    color: colors.themeColors.text_color_secondary,
                },
            },
            pointer: {
                show: getDuty() >= 0,
                offsetCenter: [0, '10%'],
                icon: 'path://M2090.36389,615.30999 L2090.36389,615.30999 C2091.48372,615.30999 2092.40383,616.194028 2092.44859,617.312956 L2096.90698,728.755929 C2097.05155,732.369577 2094.2393,735.416212 2090.62566,735.56078 C2090.53845,735.564269 2090.45117,735.566014 2090.36389,735.566014 L2090.36389,735.566014 C2086.74736,735.566014 2083.81557,732.63423 2083.81557,729.017692 C2083.81557,728.930412 2083.81732,728.84314 2083.82081,728.755929 L2088.2792,617.312956 C2088.32396,616.194028 2089.24407,615.30999 2090.36389,615.30999 Z',
                length: '116%',
                itemStyle: {
                    color: colors.themeColors.context_color,
                },
            },
            anchor: {
                show: getDuty() >= 0,
                size: 15,
                itemStyle: {
                    borderWidth: 2,
                    borderColor: colors.themeColors.context_hover,
                    color: colors.themeColors.context_color,
                },
            },
            axisLabel: {
                distance: deviceStore.getREMSize(0.9),
                color: colors.themeColors.text_color_secondary,
                fontSize: deviceStore.getREMSize(0.8),
            },
            title: {
                show: false,
            },
            detail: {
                show: getDuty() >= 0,
                valueAnimation: true,
                fontSize: deviceStore.getREMSize(3),
                color: colors.themeColors.text_color,
                offsetCenter: [0, '60%'],
                formatter: function (value) {
                    return `${value}%`
                },
            },
            silent: true,
            data: dutyGaugeData,
        },
        {
            id: 'rpmText',
            type: 'gauge',
            pointer: {
                show: false,
            },
            progress: {
                show: false,
            },
            axisLine: {
                show: false,
            },
            splitLine: {
                show: false,
            },
            axisTick: {
                show: false,
            },
            axisLabel: {
                show: false,
            },
            title: {
                show: false,
            },
            detail: {
                show: getRPMs() >= 0,
                valueAnimation: true,
                fontSize: getDuty() >= 0 ? deviceStore.getREMSize(1.5) : deviceStore.getREMSize(3),
                color: colors.themeColors.text_color,
                offsetCenter: [0, '80%'],
                formatter: function (value) {
                    return `${value} rpm`
                },
            },
            silent: true,
            data: rpmGaugeData,
        },
    ],
    animation: true,
    animationDuration: 300,
    animationDurationUpdate: 300,
}

const setGraphData = () => {
    dutyGaugeData[0].value = getDuty()
    rpmGaugeData[0].value = getRPMs()
}
setGraphData()

const defaultGaugeChart = ref<InstanceType<typeof VChart> | null>(null)

watch(currentDeviceStatus, () => {
    setGraphData()
    defaultGaugeChart.value?.setOption({
        series: [
            { id: 'gaugeChart', data: dutyGaugeData },
            { id: 'rpmText', data: rpmGaugeData },
        ],
    })
})

watch(settingsStore.allUIDeviceSettings, () => {
    const dutyColor = getDutySensorColor()
    // @ts-ignore
    option.series[0].progress.itemStyle.color = dutyColor
    defaultGaugeChart.value?.setOption({
        series: [{ id: 'gaugeChart', progress: { itemStyle: { color: dutyColor } } }],
    })
})
</script>

<template>
    <v-chart
        class="control-graph"
        ref="defaultGaugeChart"
        :option="option"
        :autoresize="true"
        :manual-update="true"
    />
</template>

<style scoped lang="scss">
.control-graph {
    height: calc(100vh - 20rem);
    width: 99.9%; // This handles an issue with the graph when the layout thinks it's too big for the container
}
</style>
