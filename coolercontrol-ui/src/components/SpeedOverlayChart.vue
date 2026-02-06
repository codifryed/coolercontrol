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
import { GridComponent, MarkPointComponent, TitleComponent } from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { UniversalTransition } from 'echarts/features'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import { FunctionType, Profile, ProfileType } from '@/models/Profile'
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { Ref, ref, toRaw, watch } from 'vue'
import { useI18n } from 'vue-i18n'

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
// We need to use the raw state to watch for changes, as the pinia reactive proxy isn't properly
// reacting to changes from Vue's shallowRef & triggerRef anymore.
const rawStore = toRaw(deviceStore.$state)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const { t } = useI18n()

const dutyMin: number = 0
const dutyMax: number = 100
const baseProfile: Profile | undefined = settingsStore.profiles.find((profile) =>
    props.profile.member_profile_uids.includes(profile.uid),
)
if (baseProfile == null) console.error('Base profile not found')
const memberProfiles: Ref<Array<Profile>> = ref(
    settingsStore.profiles.filter((profile) =>
        props.profile.member_profile_uids.includes(profile.uid),
    ),
)
if (baseProfile!.p_type === ProfileType.Mix) {
    // member profiles are either the Overlay Profile's base profile,
    // or if the base profile is a Mix Profile, we draw the members are that of the Mix profile.
    memberProfiles.value = settingsStore.profiles.filter((profile) =>
        baseProfile!.member_profile_uids.includes(profile.uid),
    )
}
let currentAxisTempMin = 0
let currentAxisTempMax = 100
let profileTempMin = 50
let profileTempMax = 50
for (const profile of memberProfiles.value) {
    for (const device of deviceStore.allDevices()) {
        if (device.uid === profile.temp_source?.device_uid) {
            if (device.info == null) {
                break
            }
            currentAxisTempMin = Math.min(currentAxisTempMin, device.info!.temp_min)
            currentAxisTempMax = Math.max(currentAxisTempMax, device.info!.temp_max)
        }
    }
    if (profile.temp_min != null) {
        profileTempMin = Math.min(profileTempMin, profile.temp_min)
    }
    if (profile.temp_max != null) {
        profileTempMax = Math.max(profileTempMax, profile.temp_max)
    }
}
if (profileTempMax > 50 && currentAxisTempMax > 100) {
    currentAxisTempMax = Math.min(currentAxisTempMax, profileTempMax)
} else if (profileTempMax == 50 && currentAxisTempMax > 100) {
    currentAxisTempMax = 100
}
const axisXTempMin: number = currentAxisTempMin
const axisXTempMax: number = currentAxisTempMax

interface LineData {
    value: number[]
}

const deviceDutyLineData: [LineData, LineData] = [{ value: [] }, { value: [] }]
// each member profile will have a tempLine and a GraphLine with the same array index
// @ts-ignore
const tempLineData: [[LineData, LineData]] = []
const graphLineData: Array<Array<LineData>> = []
const graphOffsetLineData: Array<Array<LineData>> = []
for (let i = 0; i < memberProfiles.value.length; i++) {
    tempLineData.push([{ value: [] }, { value: [] }])
    graphLineData.push([])
    graphOffsetLineData.push([])
}

const getDeviceDutyLineColor = (): string => {
    return (
        settingsStore.allUIDeviceSettings
            .get(props.currentDeviceUID)
            ?.sensorsAndChannels.get(props.currentSensorName)!.color ?? colors.themeColors.yellow
    )
}
const getTempLineColor = (profileIndex: number): string => {
    const profile = memberProfiles.value[profileIndex]
    if (profile.temp_source == null) {
        return colors.themeColors.yellow
    }
    return (
        settingsStore.allUIDeviceSettings
            .get(profile.temp_source.device_uid)
            ?.sensorsAndChannels.get(profile.temp_source.temp_name)!.color ??
        colors.themeColors.yellow
    )
}

const getDuty = (): number => {
    return Number(
        deviceStore.currentDeviceStatus.get(props.currentDeviceUID)?.get(props.currentSensorName)
            ?.duty ?? 0,
    )
}

const getTemp = (profileIndex: number): number => {
    const profile = memberProfiles.value[profileIndex]
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

const getDutyPosition = (duty: number): string => {
    return duty < 91 ? 'top' : 'bottom'
}
const calcSmoothness = (profileIndex: number): number => {
    const profile = memberProfiles.value[profileIndex]
    const fun = settingsStore.functions.find((f) => f.uid === profile.function_uid)
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return 0.0
    } else {
        return 0.1
    }
}
const calcLineShadowColor = (profileIndex: number): string => {
    const profile = memberProfiles.value[profileIndex]
    const fun = settingsStore.functions.find((f) => f.uid === profile.function_uid)
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return colors.themeColors.bg_one
    } else {
        return colors.themeColors.accent
    }
}
const calcLineShadowSize = (profileIndex: number): number => {
    const profile = memberProfiles.value[profileIndex]
    const fun = settingsStore.functions.find((f) => f.uid === profile.function_uid)
    if (fun == null || fun.f_type === FunctionType.Identity) {
        return 10
    } else {
        return 20
    }
}

/**
 * This function interpolates an offset profile to a given duty and outputs the calculated offset
 * It is direct port of the Rust function in the backend.
 */
const interpolate_offset_profile = (offset_profile: [number, number][], duty: number): number => {
    let step_below = offset_profile[0]
    let step_above = offset_profile[offset_profile.length - 1]
    for (const step of offset_profile) {
        if (step[0] <= duty) {
            step_below = step
        }
        if (step[0] >= duty) {
            step_above = step
            break
        }
    }
    if (step_below[0] === step_above[0]) {
        return step_below[1] // temp matches exactly, no duty calculation needed
    }
    const step_below_duty = step_below[0]
    const step_below_offset = step_below[1]
    const step_above_duty = step_above[0]
    const step_above_offset = step_above[1]
    // no rounding to make sure we have smooth point transitions
    return (
        step_below_offset +
        ((duty - step_below_duty) / (step_above_duty - step_below_duty)) *
            (step_above_offset - step_below_offset)
    )
}
const calcDutyFromBaseProfileDuty = (baseProfileDuty: number): number => {
    const offset = interpolate_offset_profile(props.profile.offset_profile, baseProfileDuty)
    // we don't limit the duty here because we need to show the near-real interpolation simulation
    return baseProfileDuty + offset
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
    // no rounding to make sure we have smooth point transitions
    return (
        step_below_duty +
        ((temp - step_below_temp) / (step_above_temp - step_below_temp)) *
            (step_above_duty - step_below_duty)
    )
}

const option = {
    title: {
        show: false,
    },
    grid: {
        show: false,
        top: deviceStore.getREMSize(0.5),
        left: 0,
        right: deviceStore.getREMSize(1.2),
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
            color: colors.themeColors.text_color_secondary,
            formatter: (value: any): string => `${value}${t('common.tempUnit')} `,
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
            color: colors.themeColors.text_color_secondary,
            formatter: (value: any): string => `${value}${t('common.percentUnit')}`,
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
    series: [],
    animation: true,
    animationDuration: 200,
    animationDurationUpdate: 200,
}

// series is dynamic and dependent on member profiles
for (let i = 0; i < memberProfiles.value.length; i++) {
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
            symbol: 'none',
            lineStyle: {
                color: colors.convertColorToRGBA(
                    getTempLineColor(i),
                    memberProfiles.value.length > 1 ? 0.05 : 0.25,
                ),
                // color: getTempLineColor(i),
                width: deviceStore.getREMSize(0.5),
                cap: 'round',
                shadowColor: calcLineShadowColor(i),
                shadowBlur: calcLineShadowSize(i),
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

// offset line(s)
for (let i = 0; i < memberProfiles.value.length; i++) {
    // @ts-ignore
    option.series.push({
        id: 'offsetLine' + i,
        type: 'line',
        smooth: calcSmoothness(i),
        symbol: 'none',
        lineStyle: {
            color: getTempLineColor(i),
            width: deviceStore.getREMSize(0.5),
            cap: 'round',
            shadowColor: calcLineShadowColor(i),
            shadowBlur: calcLineShadowSize(i),
        },
        emphasis: {
            disabled: true,
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
        data: graphOffsetLineData[i],
        z: 1,
        silent: true,
    })
}

// @ts-ignore
option.series.push({
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
                color: colors.convertColorToRGBA(getDeviceDutyLineColor(), 0.3),
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
})

const setGraphData = (profileIndex: number) => {
    const temp = getTemp(profileIndex)
    tempLineData[profileIndex][0].value = [temp, dutyMin]
    tempLineData[profileIndex][1].value = [temp, dutyMax]
    graphLineData[profileIndex].length = 0
    const profile = memberProfiles.value[profileIndex]
    if (profile.speed_profile.length > 1) {
        const firstPoint = profile.speed_profile[0]
        if (firstPoint[0] > axisXTempMin) {
            graphLineData[profileIndex].push({ value: [axisXTempMin, firstPoint[1]] })
        }
        for (const point of profile.speed_profile) {
            graphLineData[profileIndex].push({ value: point })
        }
        const lastPoint = profile.speed_profile[profile.speed_profile.length - 1]
        if (lastPoint[0] < axisXTempMax) {
            graphLineData[profileIndex].push({ value: [axisXTempMax, lastPoint[1]] })
        }
    }
}
for (let i = 0; i < memberProfiles.value.length; i++) {
    setGraphData(i)
}
const setGraphOffsetData = (profileIndex: number) => {
    graphOffsetLineData[profileIndex].length = 0
    const profile = memberProfiles.value[profileIndex]
    if (
        profile.speed_profile.length > 1 &&
        props.profile.offset_profile != null &&
        props.profile.offset_profile.length > 0
    ) {
        if (props.profile.offset_profile.length > 1) {
            // for graph offsets we need to calculate every signle duty point, as the duty can go
            // up and down outside the original speed profile points
            let currentTemp = axisXTempMin
            while (currentTemp < axisXTempMax) {
                graphOffsetLineData[profileIndex].push({
                    value: [
                        currentTemp,
                        calcDutyFromBaseProfileDuty(
                            interpolate_profile(profile.speed_profile, currentTemp),
                        ),
                    ],
                })
                currentTemp += 0.1
            }
            return
        }
        const firstPoint = profile.speed_profile[0]
        if (firstPoint[0] > axisXTempMin) {
            graphOffsetLineData[profileIndex].push({
                value: [axisXTempMin, calcDutyFromBaseProfileDuty(firstPoint[1])],
            })
        }
        // We calculate the offset profile based on the base profile's speed profile and allow it
        // to go out of bounds. This is by far the easiest way to draw a realistic offset line
        // and keep using line smoothing.
        profile.speed_profile.forEach((point) => {
            graphOffsetLineData[profileIndex].push({
                value: [point[0], calcDutyFromBaseProfileDuty(point[1])],
            })
        })
        const lastPoint = profile.speed_profile[profile.speed_profile.length - 1]
        if (lastPoint[0] < axisXTempMax) {
            graphOffsetLineData[profileIndex].push({
                value: [axisXTempMax, calcDutyFromBaseProfileDuty(lastPoint[1])],
            })
        }
    }
}
for (let i = 0; i < memberProfiles.value.length; i++) {
    setGraphOffsetData(i)
}

const setDutyData = (): number => {
    const duty = getDuty()
    deviceDutyLineData[0].value = [axisXTempMin, duty]
    deviceDutyLineData[1].value = [axisXTempMax, duty]
    return duty
}
setDutyData()

const overlayGraph = ref<InstanceType<typeof VChart> | null>(null)

watch(rawStore.currentDeviceStatus, () => {
    const duty = setDutyData()
    overlayGraph.value?.setOption({
        series: [
            {
                id: 'dutyLine',
                data: deviceDutyLineData,
                markPoint: {
                    data: [{ coord: [5, duty], value: duty }],
                    label: { position: getDutyPosition(duty) },
                },
            },
        ],
    })
    for (let i = 0; i < memberProfiles.value.length; i++) {
        const temp = getTemp(i)
        tempLineData[i][0].value = [temp, dutyMin]
        tempLineData[i][1].value = [temp, dutyMax]
        overlayGraph.value?.setOption({
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
    const dutyLineColor = getDeviceDutyLineColor()
    overlayGraph.value?.setOption({
        series: [
            {
                id: 'dutyLine',
                lineStyle: { color: dutyLineColor },
                markPoint: { label: { color: dutyLineColor } },
            },
        ],
    })
    for (let i = 0; i < memberProfiles.value.length; i++) {
        const tempLineColor = getTempLineColor(i)
        // @ts-ignore
        option.series[i * 2].lineStyle.color = tempLineColor
        // @ts-ignore
        option.series[i * 2].markPoint.label.color = tempLineColor
        // @ts-ignore
        option.series[i * 2 + 1].lineStyle.color = tempLineColor
        overlayGraph.value?.setOption({
            series: [
                {
                    id: 'tempLine' + i,
                    lineStyle: { color: tempLineColor },
                    markPoint: { label: { color: tempLineColor } },
                },
                {
                    id: 'graphLine' + i,
                    lineStyle: { color: tempLineColor },
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
        ref="overlayGraph"
        :option="option"
        :autoresize="true"
        :manual-update="true"
    />
</template>

<style scoped lang="scss">
#control-graph {
    height: max(calc(100vh - 4rem), 20rem);
}
</style>
