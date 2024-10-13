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
import { Profile } from '@/models/Profile'
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { Ref, ref, watch } from 'vue'

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
const memberProfiles: Ref<Array<Profile>> = ref(
    settingsStore.profiles.filter((profile) =>
        props.profile.member_profile_uids.includes(profile.uid),
    ),
)

interface LineData {
    value: number[]
}

const deviceDutyLineData: [LineData, LineData] = [{ value: [] }, { value: [] }]
// each member profile will have a tempLine and a GraphLine with the same array index
// @ts-ignore
const tempLineData: [[LineData, LineData]] = []
const graphLineData: Array<Array<LineData>> = []
for (let i = 0; i < memberProfiles.value.length; i++) {
    tempLineData.push([{ value: [] }, { value: [] }])
    graphLineData.push([])
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
// todo: this could be useful later:
// const getTempLineColorWithAlpha = (profileIndex: number, hexAlpha: string): string => {
//     const color: string = getTempLineColor(profileIndex)
//     if (color.startsWith('rgb(')) {
//         const decimalAlpha = parseInt(hexAlpha, 16) / 255
//         return color.replace('rgb', 'rgba').replace(')', `,${decimalAlpha})`)
//     } else {
//         return `${color}${hexAlpha}`
//     }
// }

const getDuty = (): number => {
    return Number(
        currentDeviceStatus.value.get(props.currentDeviceUID)?.get(props.currentSensorName)?.duty ??
            0,
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

const option = {
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
            smooth: 0.03,
            symbol: 'none',
            lineStyle: {
                color: getTempLineColor(i),
                width: deviceStore.getREMSize(0.5),
                cap: 'round',
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
    id: 'dutyLine',
    type: 'line',
    smooth: false,
    symbol: 'none',
    lineStyle: {
        color: getDeviceDutyLineColor(),
        width: deviceStore.getREMSize(0.3),
        type: 'solid',
        cap: 'round',
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
})

const setGraphData = (profileIndex: number) => {
    const temp = getTemp(profileIndex)
    tempLineData[profileIndex][0].value = [temp, dutyMin]
    tempLineData[profileIndex][1].value = [temp, dutyMax]
    graphLineData[profileIndex].length = 0
    const profile = memberProfiles.value[profileIndex]
    if (profile.speed_profile.length > 1) {
        for (const point of profile.speed_profile) {
            graphLineData[profileIndex].push({ value: point })
        }
    }
}
for (let i = 0; i < memberProfiles.value.length; i++) {
    setGraphData(i)
}

const setDutyData = (): number => {
    const duty = getDuty()
    deviceDutyLineData[0].value = [axisXTempMin, duty]
    deviceDutyLineData[1].value = [axisXTempMax, duty]
    return duty
}
setDutyData()

const mixGraph = ref<InstanceType<typeof VChart> | null>(null)

watch(currentDeviceStatus, () => {
    const duty = setDutyData()
    mixGraph.value?.setOption({
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
    const dutyLineColor = getDeviceDutyLineColor()
    mixGraph.value?.setOption({
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
                },
            ],
        })
    }
})
</script>

<template>
    <v-chart
        class="mix-graph pt-6 pr-11 pl-4 pb-6"
        ref="mixGraph"
        :option="option"
        :autoresize="true"
        :manual-update="true"
    />
</template>

<style scoped lang="scss">
.mix-graph {
    height: max(calc(100vh - 3.875rem), 40rem);
}
</style>
