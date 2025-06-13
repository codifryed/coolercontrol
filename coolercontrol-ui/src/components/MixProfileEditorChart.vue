<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2025  Guy Boldon and contributors
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
import {
    DataZoomComponent,
    GraphicComponent,
    GridComponent,
    MarkPointComponent,
} from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { UniversalTransition } from 'echarts/features'
import VChart from 'vue-echarts'
import { CanvasRenderer } from 'echarts/renderers'
import { useDeviceStore } from '@/stores/DeviceStore'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { ref, watch } from 'vue'
import { ProfileMixFunctionType, Profile, FunctionType } from '@/models/Profile'

echarts.use([
    GridComponent,
    LineChart,
    CanvasRenderer,
    UniversalTransition,
    GraphicComponent,
    DataZoomComponent,
    MarkPointComponent,
])

interface Props {
    profiles: Array<Profile>
    mixFunctionType: ProfileMixFunctionType
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()

//--------------------------------------------------------------------------------------------------

const axisXTempMin: number = 0
const axisXTempMax: number = 100
const dutyMin: number = 0
const dutyMax: number = 100

interface LineData {
    value: number[]
}

// each member profile will have a tempLine and a GraphLine with the same array index
// @ts-ignore
const tempLineData: [[LineData, LineData]] = []
const graphLineData: Array<Array<LineData>> = []
for (let i = 0; i < props.profiles.length; i++) {
    tempLineData.push([{ value: [] }, { value: [] }])
    graphLineData.push([])
}
const calculatedDutyLineData: [LineData, LineData] = [{ value: [] }, { value: [] }]

const getTempLineColor = (profileIndex: number): string => {
    const profile = props.profiles[profileIndex]
    if (profile.temp_source == null) {
        return colors.themeColors.accent
    }
    return (
        settingsStore.allUIDeviceSettings
            .get(profile.temp_source.device_uid)
            ?.sensorsAndChannels.get(profile.temp_source.temp_name)!.color ??
        colors.themeColors.accent
    )
}
const calcSmoothness = (profileIndex: number): number => {
    const profile = props.profiles[profileIndex]
    const fun = settingsStore.functions.find((f) => f.uid === profile.function_uid)
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return 0.0
    } else {
        return 0.3
    }
}
const calcLineShadowColor = (profileIndex: number): string => {
    const profile = props.profiles[profileIndex]
    const fun = settingsStore.functions.find((f) => f.uid === profile.function_uid)
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return colors.themeColors.bg_one
    } else {
        return colors.themeColors.accent
    }
}
const calcLineShadowSize = (profileIndex: number): number => {
    const profile = props.profiles[profileIndex]
    const fun = settingsStore.functions.find((f) => f.uid === profile.function_uid)
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return 10
    } else {
        return 20
    }
}

const option = {
    grid: {
        show: false,
        top: deviceStore.getREMSize(0.75),
        left: 0,
        right: deviceStore.getREMSize(0.9),
        bottom: 0,
        containLabel: true,
    },
    xAxis: {
        min: axisXTempMin,
        max: axisXTempMax,
        type: 'value',
        splitNumber: 10,
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.95),
            formatter: '{value}° ',
        },
        axisLine: {
            lineStyle: {
                color: colors.themeColors.text_color,
                width: 1,
            },
        },
        splitLine: {
            lineStyle: {
                color: colors.themeColors.border,
                width: 0.5,
                type: 'dotted',
            },
        },
    },
    yAxis: {
        min: dutyMin,
        max: dutyMax,
        type: 'value',
        splitNumber: 10,
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.95),
            formatter: '{value}%',
        },
        axisLine: {
            lineStyle: {
                color: colors.themeColors.text_color,
                width: 1,
            },
        },
        splitLine: {
            lineStyle: {
                color: colors.themeColors.border,
                width: 0.5,
                type: 'dotted',
            },
        },
    },
    dataZoom: [
        {
            type: 'inside',
            xAxisIndex: 0,
            filterMode: 'none',
            preventDefaultMouseMove: false,
        },
    ],
    series: [],
    animation: true,
    animationDuration: 300,
    animationDurationUpdate: 300,
}

const getTemp = (profileIndex: number): number => {
    const profile = props.profiles[profileIndex]
    if (profile.temp_source == null) {
        return 0
    }
    const tempValue = deviceStore.currentDeviceStatus
        .get(profile.temp_source.device_uid)
        ?.get(profile.temp_source.temp_name)?.temp
    if (tempValue == null) {
        return 0
    }
    return Number(tempValue)
}

/**
 * Calculate a simple duty (no function settings) from the member profiles and MixFunctionType
 */
const calculateDuty = (): number => {
    const allDuties: number[] = []
    for (let i = 0; i < props.profiles.length; i++) {
        const temp = getTemp(i)
        const profile = props.profiles[i]
        const duty = interpolate_profile(profile.speed_profile, temp)
        allDuties.push(duty)
    }
    switch (props.mixFunctionType) {
        case ProfileMixFunctionType.Avg:
            return allDuties.reduce((a, b) => a + b, 0) / allDuties.length
        case ProfileMixFunctionType.Max:
            return Math.max(...allDuties)
        case ProfileMixFunctionType.Min:
            return Math.min(...allDuties)
    }
}

/**
 * This function interpolates a speed profile to a given temperature and outputs the calculated duty
 * It is direct port of the Rust function in the backend.
 */
const interpolate_profile = (speed_profile: [number, number][], temp: number): number => {
    let step_below = speed_profile[0]
    let step_above = speed_profile[speed_profile.length - 1]
    for (const step of speed_profile) {
        if (step[0] <= temp) {
            step_below = step
        }
        if (step[0] >= temp) {
            step_above = step
            break
        }
    }
    if (step_below[0] === step_above[0]) {
        return step_below[1] // temp matches exactly, no duty calculation needed
    }
    const step_below_temp = step_below[0]
    const step_below_duty = step_below[1]
    const step_above_temp = step_above[0]
    const step_above_duty = step_above[1]
    return Math.round(
        step_below_duty +
            ((temp - step_below_temp) / (step_above_temp - step_below_temp)) *
                (step_above_duty - step_below_duty),
    )
}

const getDutyPosition = (duty: number): string => {
    return duty < 91 ? 'top' : 'bottom'
}

// series is dynamic and dependent on member profiles
for (let i = 0; i < props.profiles.length; i++) {
    option.series.push(
        // @ts-ignore
        {
            id: 'tempLine' + i,
            type: 'line',
            smooth: false,
            symbol: 'none',
            lineStyle: {
                color: getTempLineColor(i),
                width: deviceStore.getREMSize(0.1),
                type: 'dashed',
                shadowColor: colors.themeColors.bg_one,
                shadowBlur: 5,
                shadowOffsetX: 0,
                shadowOffsetY: 0,
            },
            emphasis: {
                disabled: true,
            },
            data: tempLineData[i],
            markPoint: {
                symbolSize: 0,
                label: {
                    position: 'top',
                    fontSize: deviceStore.getREMSize(1.0),
                    color: getTempLineColor(i),
                    rotate: 90,
                    offset: [0, -2],
                    formatter: (params: any): string => {
                        if (params.value == null) return ''
                        return Number(params.value).toFixed(1) + '°'
                    },
                    shadowColor: colors.themeColors.bg_one,
                    shadowBlur: 10,
                },
                data: [
                    {
                        coord: [getTemp(i), 95],
                        value: getTemp(i),
                    },
                ],
            },
            z: 10,
            silent: true,
        },
        {
            id: 'graphLine' + i,
            type: 'line',
            smooth: calcSmoothness(i),
            symbol: 'circle',
            itemStyle: {
                color: getTempLineColor(i),
                borderColor: getTempLineColor(i),
                borderWidth: 3,
            },
            lineStyle: {
                color: getTempLineColor(i),
                width: 2,
                type: 'solid',
                cap: 'round',
                shadowColor: calcLineShadowColor(i),
                shadowBlur: calcLineShadowSize(i),
            },
            areaStyle: {
                color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
                    {
                        offset: 0,
                        color: colors.convertColorToRGBA(getTempLineColor(i), 0.2),
                    },
                    {
                        offset: 1,
                        color: colors.convertColorToRGBA(getTempLineColor(i), 0.0),
                    },
                ]),
                opacity: 1.0,
            },
            emphasis: {
                disabled: true,
            },
            data: graphLineData[i],
            z: 1,
            silent: true,
        },
    )
}
// @ts-ignore
option.series.push({
    id: 'calculatedDutyLine',
    type: 'line',
    smooth: false,
    symbol: 'none',
    lineStyle: {
        color: `${colors.themeColors.accent}`,
        width: 4,
        type: 'solid',
    },
    emphasis: {
        disabled: true,
    },
    data: calculatedDutyLineData,
    markPoint: {
        symbolSize: 0,
        label: {
            position: getDutyPosition(calculateDuty()),
            fontSize: deviceStore.getREMSize(1.0),
            color: colors.themeColors.accent,
            formatter: (params: any): string => {
                if (params.value == null) return ''
                return Number(params.value).toFixed(0) + '%'
            },
        },
        data: [
            {
                coord: [5, calculateDuty()],
                value: calculateDuty(),
            },
        ],
    },
    areaStyle: {
        color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
            {
                offset: 0,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.3),
            },
            {
                offset: 1,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.0),
            },
        ]),
        opacity: 1.0,
    },
    z: 100,
    silent: true,
})

const setGraphData = (profileIndex: number) => {
    const temp = getTemp(profileIndex)
    tempLineData[profileIndex][0].value = [temp, dutyMin]
    tempLineData[profileIndex][1].value = [temp, dutyMax]
    graphLineData[profileIndex].length = 0
    const profile = props.profiles[profileIndex]
    if (profile.speed_profile.length > 1) {
        for (const point of profile.speed_profile) {
            graphLineData[profileIndex].push({ value: point })
        }
    }
}
for (let i = 0; i < props.profiles.length; i++) {
    setGraphData(i)
}

const setCalculatedDutyLine = (): number => {
    const duty = calculateDuty()
    calculatedDutyLineData[0].value = [axisXTempMin, duty]
    calculatedDutyLineData[1].value = [axisXTempMax, duty]
    return duty
}
setCalculatedDutyLine()

//--------------------------------------------------------------------------------------------------
const mixGraph = ref<InstanceType<typeof VChart> | null>(null)

watch(currentDeviceStatus, () => {
    const duty = setCalculatedDutyLine()
    mixGraph.value?.setOption({
        series: [
            {
                id: 'calculatedDutyLine',
                data: calculatedDutyLineData,
                markPoint: {
                    data: [{ coord: [5, duty], value: duty }],
                    label: { position: getDutyPosition(duty) },
                },
            },
        ],
    })
    for (let i = 0; i < props.profiles.length; i++) {
        const temp = getTemp(i)
        tempLineData[i][0].value = [temp, dutyMin]
        tempLineData[i][1].value = [temp, dutyMax]
        mixGraph.value?.setOption({
            series: [
                {
                    id: 'tempLine' + i,
                    data: tempLineData[i],
                    markPoint: { data: [{ coord: [temp, 95], value: temp }] },
                },
            ],
        })
    }
})

watch(settingsStore.allUIDeviceSettings, () => {
    for (let i = 0; i < props.profiles.length; i++) {
        const tempLineColor = getTempLineColor(i)
        // @ts-ignore
        option.series[i * 2].lineStyle.color = tempLineColor
        // @ts-ignore
        option.series[i * 2].markPoint.label.color = tempLineColor
        // @ts-ignore
        option.series[i * 2 + 1].lineStyle.color = tempLineColor
        mixGraph.value?.setOption({
            series: [
                {
                    id: 'tempLine' + i,
                    lineStyle: { color: tempLineColor },
                    markPoint: { label: { color: tempLineColor } },
                },
                {
                    id: 'graphLine' + i,
                    lineStyle: { color: tempLineColor },
                    itemStyle: {
                        color: tempLineColor,
                        borderColor: tempLineColor,
                    },
                    areaStyle: {
                        color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
                            {
                                offset: 0,
                                color: colors.convertColorToRGBA(tempLineColor, 0.2),
                            },
                            {
                                offset: 1,
                                color: colors.convertColorToRGBA(tempLineColor, 0.0),
                            },
                        ]),
                    },
                },
            ],
        })
    }
})
</script>

<template>
    <v-chart
        id="control-graph"
        class="pt-6 pr-11 pl-4 pb-6"
        ref="mixGraph"
        :option="option"
        :autoresize="true"
        :manual-update="true"
    />
</template>

<style scoped lang="scss">
#control-graph {
    height: max(95vh, 20rem);
    //width: max(calc(90vw - 17rem), 20rem);
}
</style>
