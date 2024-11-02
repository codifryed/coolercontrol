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
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import {onMounted, Ref, ref, watch, watchEffect} from 'vue'

echarts.use([CanvasRenderer, GaugeChart])

interface Props {
    duty?: number
    currentDeviceUID: UID
    currentSensorName: string
    defaultProfile?: boolean
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
const rpmGaugeData: Array<GaugeData> = [{ value: -1 }]
const fixedDutyGaugeData: Array<GaugeData> = [{ value: 0 }]
const getDutySensorColor = (): string => {
    return (
        settingsStore.allUIDeviceSettings
            .get(props.currentDeviceUID)
            ?.sensorsAndChannels.get(props.currentSensorName)!.color ?? colors.themeColors.yellow
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

const gaugeBasePx: Ref<number> = ref(deviceStore.getREMSize(5))
const getFixedDuty = (): number => props.duty ?? 0
const isDefaultProfile = (): boolean => props.defaultProfile ?? false

const option = {
    series: [
        {
            id: 'gaugeChart',
            type: 'gauge',
            min: dutyMin,
            max: dutyMax,
            radius: '75%',
            progress: {
                show: true,
                width: gaugeBasePx.value,
                overlap: false,
                itemStyle: {
                    color: getDutySensorColor(),
                },
            },
            axisLine: {
                lineStyle: {
                    width: gaugeBasePx.value,
                    color: [[1, colors.themeColors.bg_two]],
                },
            },
            axisTick: {
                show: true,
                length: gaugeBasePx.value,
                distance: -gaugeBasePx.value,
                lineStyle: {
                    color: colors.themeColors.text_color,
                },
            },
            splitLine: {
                length: gaugeBasePx.value,
                distance: -gaugeBasePx.value,
                lineStyle: {
                    color: colors.themeColors.text_color,
                },
            },
            pointer: {
                show: getDuty() >= 0,
                length: '70%',
                width: '1%',
                itemStyle: {
                    color: getDutySensorColor(),
                },
            },
            axisLabel: {
                distance: gaugeBasePx.value,
                color: colors.themeColors.text_color,
                fontSize: gaugeBasePx.value,
            },
            title: {
                show: true,
                offsetCenter: [0, '90%'],
            },
            detail: {
                show: getDuty() >= 0,
                valueAnimation: true,
                fontSize: deviceStore.getREMSize(3),
                color: colors.themeColors.text_color,
                offsetCenter: [0, '60%'],
                formatter: function (value: string) {
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
                formatter: function (value: string) {
                    return `${value} rpm`
                },
            },
            silent: true,
            data: rpmGaugeData,
        },
        {
            id: 'fixedPointer',
            type: 'gauge',
            z: 2,
            pointer: {
                show: !isDefaultProfile(),
                showAbove: true,
                offsetCenter: [0, '15%'],
                icon: 'path://M2090.36389,615.30999 L2090.36389,615.30999 C2091.48372,615.30999 2092.40383,616.194028 2092.44859,617.312956 L2096.90698,728.755929 C2097.05155,732.369577 2094.2393,735.416212 2090.62566,735.56078 C2090.53845,735.564269 2090.45117,735.566014 2090.36389,735.566014 L2090.36389,735.566014 C2086.74736,735.566014 2083.81557,732.63423 2083.81557,729.017692 C2083.81557,728.930412 2083.81732,728.84314 2083.82081,728.755929 L2088.2792,617.312956 C2088.32396,616.194028 2089.24407,615.30999 2090.36389,615.30999 Z',
                width: 15,
                length: '123%',
                itemStyle: {
                    color: colors.themeColors.accent,
                },
            },
            anchor: {
                show: !isDefaultProfile(),
                size: 35,
                itemStyle: {
                    borderWidth: 2,
                    borderColor: colors.themeColors.accent,
                    color: colors.themeColors.accent,
                },
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
            detail: {
                show: false,
            },
            title: {
                show: false,
            },
            silent: true,
            data: fixedDutyGaugeData,
        },
    ],
    animation: true,
    animationDuration: 200,
    animationDurationUpdate: 200,
}

const setGraphData = () => {
    dutyGaugeData[0].value = getDuty()
    rpmGaugeData[0].value = getRPMs()
    fixedDutyGaugeData[0].value = getFixedDuty()
}
setGraphData()

const fixedGaugeChart = ref<InstanceType<typeof VChart> | null>(null)

watch(currentDeviceStatus, () => {
    setGraphData()
    fixedGaugeChart.value?.setOption({
        series: [
            { id: 'gaugeChart', data: dutyGaugeData },
            { id: 'rpmText', data: rpmGaugeData },
            { id: 'fixedPointer', data: fixedDutyGaugeData },
        ],
    })
})

watch(settingsStore.allUIDeviceSettings, () => {
    const dutyColor = getDutySensorColor()
    // @ts-ignore
    option.series[0].progress.itemStyle.color = dutyColor
    fixedGaugeChart.value?.setOption({
        series: [{ id: 'gaugeChart', progress: { itemStyle: { color: dutyColor } } }],
    })
})

onMounted(() => {
    const getGaugeBase = (rect: DOMRect): number => Math.min(rect.width, rect.height)
    const gaugeEl: HTMLElement = fixedGaugeChart.value?.$el!
    gaugeBasePx.value = getGaugeBase(gaugeEl.getBoundingClientRect())

    const resizeObserver = new ResizeObserver((_) => {
        gaugeBasePx.value = getGaugeBase(gaugeEl.getBoundingClientRect())
        fixedGaugeChart.value?.setOption({
            series: [
                {
                    id: 'gaugeChart',
                    progress: {
                        width: gaugeBasePx.value * 0.1,
                    },
                    axisLine: {
                        lineStyle: {
                            width: gaugeBasePx.value * 0.1,
                        },
                    },
                    axisTick: {
                        length: gaugeBasePx.value * 0.014,
                        distance: -gaugeBasePx.value * 0.114,
                    },
                    splitLine: {
                        length: gaugeBasePx.value * 0.028,
                        distance: -gaugeBasePx.value * 0.128,
                    },
                    axisLabel: {
                        distance: gaugeBasePx.value * 0.04,
                        fontSize: Math.min(deviceStore.getREMSize(1.25), gaugeBasePx.value * 0.025),
                    },
                    detail: {
                        fontSize: Math.min(deviceStore.getREMSize(4.5), gaugeBasePx.value * 0.1),
                    },
                },
                {
                    id: 'rpmText',
                    detail: {
                        fontSize:
                            getDuty() >= 0
                                ? Math.min(deviceStore.getREMSize(3), gaugeBasePx.value * 0.05)
                                : Math.min(deviceStore.getREMSize(4), gaugeBasePx.value * 0.08),
                    },
                },
                {
                    id: 'fixedPointer',
                    pointer: {
                        width: Math.max(gaugeBasePx.value * 0.015, 10),
                    },
                    anchor: {
                        size: Math.max(gaugeBasePx.value * 0.04, 15),
                    },
                },
            ],
        })
    })
    resizeObserver.observe(gaugeEl)

    watch(settingsStore.allUIDeviceSettings, () => {
        const sensorColor = getDutySensorColor()
        // @ts-ignore
        option.series[0].progress.itemStyle.color = sensorColor
        fixedGaugeChart.value?.setOption({
            series: [
                {
                    id: 'gaugeChart',
                    progress: { itemStyle: { color: sensorColor } },
                    pointer: { itemStyle: { color: sensorColor } },
                },
            ],
        })
    })
})
</script>

<template>
    <v-chart
        class="control-graph"
        ref="fixedGaugeChart"
        :option="option"
        :autoresize="true"
        :manual-update="true"
    />
</template>

<style scoped lang="scss">
.control-graph {
    height: min(calc(100vw - 20rem), calc(90vh));
    width: 100%;
}
</style>
