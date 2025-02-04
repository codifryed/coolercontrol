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
import { GridComponent, MarkPointComponent, TitleComponent } from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { UniversalTransition } from 'echarts/features'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import { FunctionType, Profile } from '@/models/Profile'
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { ref, watch } from 'vue'
import { useRouter } from 'vue-router'

echarts.use([
    GridComponent,
    MarkPointComponent,
    LineChart,
    CanvasRenderer,
    UniversalTransition,
    TitleComponent,
])

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
const router = useRouter()

// set min & max dependent of temp source range:
let currentTempSourceMin = 0
let currentTempSourceMax = 100
for (const device of deviceStore.allDevices()) {
    if (device.uid === props.profile.temp_source?.device_uid) {
        if (device.info == null) {
            break
        }
        currentTempSourceMin = device.info!.temp_min
        currentTempSourceMax = device.info!.temp_max
    }
}
const axisXTempMin: number = currentTempSourceMin
const axisXTempMax: number = currentTempSourceMax
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

const fun = settingsStore.functions.find((f) => f.uid === props.profile.function_uid)
const calcSmoothness = (): number => {
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return 0.0
    } else {
        return 0.3
    }
}
const calcLineShadowColor = (): string => {
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return colors.themeColors.bg_one
    } else {
        return colors.themeColors.accent
    }
}
const calcLineShadowSize = (): number => {
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return 10
    } else {
        return 20
    }
}

const profileTitle = (): string => {
    let title = `Applied Profile: ${props.profile.name}`
    if (deviceStore.isSafariWebKit()) {
        // add some extra length for WebKit to keep default function text all linkable
        title = title + '                      '
    }
    return title
}
const option = {
    title: {
        show: true,
        text: profileTitle(),
        link: '',
        target: 'self',
        top: '5%',
        left: '5%',
        textStyle: {
            color: colors.themeColors.text_color,
            fontStyle: 'italic',
            fontSize: '1.2rem',
            textShadowColor: colors.themeColors.bg_one,
            textShadowBlur: 10,
        },
        triggerEvent: true,
        // z: 0,
    },
    grid: {
        show: false,
        top: deviceStore.getREMSize(0.5),
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
                color: colors.themeColors.border,
                width: 1,
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
                type: 'dotted',
                width: 1,
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
                width: deviceStore.getREMSize(0.3),
                type: 'solid',
                cap: 'round',
                shadowColor: colors.themeColors.bg_one,
                shadowBlur: 10,
            },
            emphasis: {
                disabled: true,
            },
            data: deviceDutyLineData,
            markPoint: {
                symbolSize: 0,
                label: {
                    position: getDutyPosition(getDuty()),
                    fontSize: deviceStore.getREMSize(1.0),
                    color: getDeviceDutyLineColor(),
                    formatter: (params: any): string => {
                        if (params.value == null) return ''
                        return Number(params.value).toFixed(0) + '%'
                    },
                    shadowColor: colors.themeColors.bg_one,
                    shadowBlur: 10,
                },
                data: [
                    {
                        coord: [5, getDuty()],
                        value: getDuty(),
                    },
                ],
            },
            areaStyle: {
                color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
                    {
                        offset: 0,
                        color: colors.convertColorToRGBA(getDeviceDutyLineColor(), 0.4),
                    },
                    {
                        offset: 1,
                        color: colors.convertColorToRGBA(getDeviceDutyLineColor(), 0.0),
                    },
                ]),
                opacity: 1.0,
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
                shadowColor: colors.themeColors.bg_one,
                shadowBlur: 5,
                shadowOffsetX: 0,
                shadowOffsetY: 0,
            },
            emphasis: {
                disabled: true,
            },
            data: tempLineData,
            markPoint: {
                symbolSize: 0,
                label: {
                    position: 'top',
                    fontSize: deviceStore.getREMSize(1.0),
                    color: getTempLineColor(),
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
            smooth: calcSmoothness(),
            symbol: 'none',
            lineStyle: {
                color: colors.themeColors.accent,
                width: deviceStore.getREMSize(0.5),
                cap: 'round',
                shadowColor: calcLineShadowColor(),
                shadowBlur: calcLineShadowSize(),
            },
            emphasis: {
                disabled: true,
            },
            areaStyle: {
                color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
                    {
                        offset: 0,
                        color: colors.convertColorToRGBA(colors.themeColors.accent, 0.5),
                    },
                    {
                        offset: 1,
                        color: colors.convertColorToRGBA(colors.themeColors.accent, 0.0),
                    },
                ]),
                opacity: 1.0,
            },
            data: graphLineData,
            z: 1,
            silent: true,
        },
    ],
    animation: true,
    animationDuration: 200,
    animationDurationUpdate: 200,
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

const handleGraphClick = (params: any): void => {
    if (params.target?.style?.text === option.title.text) {
        // handle click on Profile Title in graph:
        router.push({ name: 'profiles', params: { profileUID: props.profile.uid } })
    }
}

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
            {
                id: 'dutyLine',
                lineStyle: { color: dutyLineColor },
                markPoint: {
                    label: {
                        color: dutyLineColor,
                    },
                },
                areaStyle: {
                    color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
                        {
                            offset: 0,
                            color: colors.convertColorToRGBA(dutyLineColor, 0.4),
                        },
                        {
                            offset: 1,
                            color: colors.convertColorToRGBA(dutyLineColor, 0.0),
                        },
                    ]),
                },
            },
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
        class="control-graph pt-6 pr-11 pl-4 pb-6"
        ref="controlGraph"
        :option="option"
        :autoresize="true"
        :manual-update="true"
        @zr:click="handleGraphClick"
    />
</template>

<style scoped lang="scss">
.control-graph {
    height: max(calc(100vh - 3.875rem), 40rem);
}
</style>
