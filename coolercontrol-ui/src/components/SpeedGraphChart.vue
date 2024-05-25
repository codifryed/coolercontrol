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
import { GridComponent, MarkPointComponent } from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { UniversalTransition } from 'echarts/features'
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

echarts.use([GridComponent, MarkPointComponent, LineChart, CanvasRenderer, UniversalTransition])

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

const axisXTempMin: number = 0
const axisXTempMax: number = 100
const dutyMin: number = 0
const dutyMax: number = 100

interface LineData {
    value: number[]
}

const deviceDutyLineData: [LineData, LineData] = [{ value: [] }, { value: [] }]
const tempLineData: [LineData, LineData] = [{ value: [] }, { value: [] }]
const graphLineData: Array<LineData> = []

const getDeviceDutyLineColor = (): string => {
    return (
        settingsStore.allUIDeviceSettings
            .get(props.currentDeviceUID)
            ?.sensorsAndChannels.get(props.currentSensorName)!.color ?? colors.themeColors.yellow
    )
}
const getTempLineColor = (): string => {
    if (props.profile.temp_source == null) {
        return colors.themeColors.yellow
    }
    return (
        settingsStore.allUIDeviceSettings
            .get(props.profile.temp_source.device_uid)
            ?.sensorsAndChannels.get(props.profile.temp_source.temp_name)!.color ??
        colors.themeColors.yellow
    )
}

const getDuty = (): number => {
    return Number(
        currentDeviceStatus.value.get(props.currentDeviceUID)?.get(props.currentSensorName)?.duty ??
            0,
    )
}

const getTemp = (): number => {
    if (props.profile.temp_source == null) {
        return 0
    }
    const tempValue = deviceStore.currentDeviceStatus
        .get(props.profile.temp_source.device_uid)
        ?.get(props.profile.temp_source.temp_name)?.temp
    if (tempValue == null) {
        return 0
    }
    return Number(tempValue)
}

const getDutyPosition = (duty: number): string => {
    return duty < 91 ? 'top' : 'bottom'
}

const option: EChartsOption = {
    grid: {
        show: false,
        top: deviceStore.getREMSize(1),
        left: deviceStore.getREMSize(1.2),
        right: deviceStore.getREMSize(0.9),
        bottom: deviceStore.getREMSize(1.5),
        containLabel: true,
    },
    xAxis: {
        min: axisXTempMin,
        max: axisXTempMax,
        name: 'temperature °C',
        nameLocation: 'middle',
        nameGap: deviceStore.getREMSize(1.8),
        nameTextStyle: {
            fontSize: deviceStore.getREMSize(0.9),
            fontWeight: 'bold',
            color: colors.themeColors.text_color,
        },
        type: 'value',
        splitNumber: 10,
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.9),
            formatter: '{value}°',
        },
        axisLine: {
            lineStyle: {
                color: colors.themeColors.text_color,
                width: 1,
            },
        },
        splitLine: {
            lineStyle: {
                color: colors.themeColors.gray_600,
                type: 'dotted',
            },
        },
    },
    yAxis: {
        min: dutyMin,
        max: dutyMax,
        name: 'duty %',
        nameLocation: 'middle',
        nameGap: deviceStore.getREMSize(2.2),
        nameTextStyle: {
            fontSize: deviceStore.getREMSize(0.9),
            fontWeight: 'bold',
            color: colors.themeColors.text_color,
        },
        type: 'value',
        splitNumber: 10,
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.9),
        },
        axisLine: {
            lineStyle: {
                color: colors.themeColors.text_color,
                width: 1,
            },
        },
        splitLine: {
            lineStyle: {
                color: colors.themeColors.gray_600,
                type: 'dotted',
            },
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
                width: deviceStore.getREMSize(0.2),
                type: 'solid',
            },
            emphasis: {
                disabled: true,
            },
            data: deviceDutyLineData,
            markPoint: {
                symbolSize: 0,
                label: {
                    position: getDutyPosition(getDuty()),
                    fontSize: deviceStore.getREMSize(0.9),
                    color: getDeviceDutyLineColor(),
                    formatter: (params: any): string => {
                        if (params.value == null) return ''
                        return Number(params.value).toFixed(0) + '%'
                    },
                },
                data: [
                    {
                        coord: [5, getDuty()],
                        value: getDuty(),
                    },
                ],
            },
            z: 100,
            silent: true,
        },
        {
            id: 'tempLine',
            type: 'line',
            smooth: false,
            symbol: 'none',
            lineStyle: {
                color: getTempLineColor(),
                width: deviceStore.getREMSize(0.1),
                type: 'dashed',
            },
            emphasis: {
                disabled: true,
            },
            data: tempLineData,
            markPoint: {
                symbolSize: 0,
                label: {
                    position: 'top',
                    fontSize: deviceStore.getREMSize(0.9),
                    color: getTempLineColor(),
                    rotate: 90,
                    offset: [0, -2],
                    formatter: (params: any): string => {
                        if (params.value == null) return ''
                        return Number(params.value).toFixed(1) + '°'
                    },
                },
                data: [
                    {
                        coord: [getTemp(), 95],
                        value: getTemp(),
                    },
                ],
            },
            z: 10,
            silent: true,
        },
        {
            id: 'GraphLine',
            type: 'line',
            smooth: 0.03,
            symbol: 'none',
            lineStyle: {
                color: {
                    type: 'linear',
                    x: 0,
                    y: 0,
                    x2: 1,
                    y2: 0,
                    colorStops: [
                        {
                            offset: 0,
                            color: `${colors.themeColors.accent}00`,
                        },
                        {
                            offset: 0.04,
                            color: `${colors.themeColors.accent}80`,
                        },
                        {
                            offset: 0.5,
                            color: `${colors.themeColors.accent}80`,
                        },
                        {
                            offset: 0.96,
                            color: `${colors.themeColors.accent}80`,
                        },
                        {
                            offset: 1,
                            color: `${colors.themeColors.accent}00`,
                        },
                    ],
                },
                width: deviceStore.getREMSize(0.5),
                cap: 'round',
            },
            emphasis: {
                disabled: true,
            },
            data: graphLineData,
            z: 1,
            silent: true,
        },
    ],
    animation: true,
    animationDuration: 300,
    animationDurationUpdate: 300,
}

const setGraphData = () => {
    const duty = getDuty()
    deviceDutyLineData[0].value = [axisXTempMin, duty]
    deviceDutyLineData[1].value = [axisXTempMax, duty]
    const temp = getTemp()
    tempLineData[0].value = [temp, dutyMin]
    tempLineData[1].value = [temp, dutyMax]
    graphLineData.length = 0
    if (props.profile.speed_profile.length > 1) {
        for (const point of props.profile.speed_profile) {
            graphLineData.push({ value: point })
        }
    }
}
setGraphData()

const controlGraph = ref<InstanceType<typeof VChart> | null>(null)

watch(currentDeviceStatus, () => {
    const duty = getDuty()
    deviceDutyLineData[0].value = [axisXTempMin, duty]
    deviceDutyLineData[1].value = [axisXTempMax, duty]
    const temp = getTemp()
    tempLineData[0].value = [temp, dutyMin]
    tempLineData[1].value = [temp, dutyMax]
    controlGraph.value?.setOption({
        series: [
            {
                id: 'dutyLine',
                data: deviceDutyLineData,
                markPoint: {
                    data: [{ coord: [5, duty], value: duty }],
                    label: { position: getDutyPosition(duty) },
                },
            },
            {
                id: 'tempLine',
                data: tempLineData,
                markPoint: { data: [{ coord: [temp, 95], value: temp }] },
            },
        ],
    })
})

watch(settingsStore.allUIDeviceSettings, () => {
    const dutyLineColor = getDeviceDutyLineColor()
    const tempLineColor = getTempLineColor()
    // @ts-ignore
    option.series[0].lineStyle.color = dutyLineColor
    // @ts-ignore
    option.series[0].markPoint.label.color = dutyLineColor
    // @ts-ignore
    option.series[1].lineStyle.color = tempLineColor
    controlGraph.value?.setOption({
        series: [
            { id: 'dutyLine', lineStyle: { color: dutyLineColor } },
            {
                id: 'tempLine',
                lineStyle: { color: tempLineColor },
                markPoint: { label: { color: tempLineColor } },
            },
        ],
    })
})
</script>

<template>
    <v-chart
        class="control-graph"
        ref="controlGraph"
        :option="option"
        :autoresize="true"
        :manual-update="true"
    />
</template>

<style scoped lang="scss">
.control-graph {
    height: calc(100vh - 8rem);
    width: 99.9%; // This handles an issue with the graph when the layout thinks it's too big for the container
}
</style>
