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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiArrowLeft, mdiInformationSlabCircleOutline } from '@mdi/js'
import {
    Function,
    FunctionType,
    Profile,
    ProfileTempSource,
    ProfileType,
} from '@/models/Profile.ts'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import * as echarts from 'echarts/core'
import {
    DataZoomComponent,
    GraphicComponent,
    GridComponent,
    MarkAreaComponent,
    MarkPointComponent,
    TooltipComponent,
    TitleComponent,
} from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { UniversalTransition } from 'echarts/features'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import type { GraphicComponentLooseOption } from 'echarts/types/dist/shared.d.ts'
import { storeToRefs } from 'pinia'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { computed, onMounted, onUnmounted, ref, Ref, watch, type WatchStopHandle } from 'vue'
import Button from 'primevue/button'
import InputNumber from 'primevue/inputnumber'
import _ from 'lodash'

echarts.use([
    GridComponent,
    LineChart,
    CanvasRenderer,
    UniversalTransition,
    TooltipComponent,
    GraphicComponent,
    MarkAreaComponent,
    MarkPointComponent,
    DataZoomComponent,
    TitleComponent,
])

interface Props {
    name: string
    tempSource: ProfileTempSource
    speedProfile: Array<[number, number]>
    tempMin?: number
    tempMax?: number
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'speedProfile', speedProfile: Array<[number, number]>): void
    (e: 'tempMin', tempMin: number): void
    (e: 'tempMax', tempMax: number): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()

interface AvailableTemp {
    deviceUID: string // needed here as well for the dropdown selector
    tempName: string
    tempFrontendName: string
    lineColor: string
    temp: string
}

interface AvailableTempSources {
    deviceUID: string
    deviceName: string
    profileMinLength: number
    profileMaxLength: number
    tempMin: number
    tempMax: number
    temps: Array<AvailableTemp>
}

interface CurrentTempSource {
    deviceUID: string
    deviceName: string
    profileMinLength: number
    profileMaxLength: number
    tempMin: number
    tempMax: number
    tempName: string
    tempFrontendName: string
    color: string
}

const currentProfile: Ref<Profile> = ref(new Profile(props.name, ProfileType.Graph))
currentProfile.value.temp_source = props.tempSource
currentProfile.value.speed_profile = props.speedProfile

const tempSources: Ref<Array<AvailableTempSources>> = ref([])
const fillTempSources = () => {
    tempSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        // For the Wizard Use case, we only need the currently selected Temp Source:
        if (
            device.status.temps.length === 0 ||
            device.info == null ||
            device.uid !== props.tempSource.device_uid
        ) {
            continue
        }
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        const deviceSource: AvailableTempSources = {
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            profileMinLength: device.info.profile_min_length,
            profileMaxLength: device.info.profile_max_length,
            tempMin: device.info.temp_min,
            tempMax: device.info.temp_max,
            temps: [],
        }
        for (const temp of device.status.temps) {
            deviceSource.temps.push({
                deviceUID: device.uid,
                tempName: temp.name,
                tempFrontendName: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                lineColor: deviceSettings.sensorsAndChannels.get(temp.name)!.color,
                temp: temp.temp.toFixed(1),
            })
        }
        if (deviceSource.temps.length === 0) {
            continue // when all of a devices temps are hidden
        }
        tempSources.value.push(deviceSource)
    }
}
fillTempSources()
const getCurrentTempSource = (
    deviceUID: string | undefined,
    tempName: string | undefined,
): CurrentTempSource | undefined => {
    if (deviceUID == null || tempName == null) {
        return undefined
    }
    const tmpDevice = tempSources.value.find((ts) => ts.deviceUID === deviceUID)
    const tmpTemp = tmpDevice?.temps.find((temp) => temp.tempName === tempName)
    if (tmpDevice != null && tmpTemp != null) {
        return {
            deviceUID: tmpDevice.deviceUID,
            deviceName: tmpDevice.deviceName,
            profileMinLength: tmpDevice.profileMinLength,
            profileMaxLength: tmpDevice.profileMaxLength,
            tempMin: tmpDevice.tempMin,
            tempMax: tmpDevice.tempMax,
            tempName: tmpTemp.tempName,
            tempFrontendName: tmpTemp.tempFrontendName,
            color: tmpTemp.lineColor,
        }
    }
    return undefined
}
let selectedTempSource: CurrentTempSource | undefined = getCurrentTempSource(
    currentProfile.value.temp_source?.device_uid,
    currentProfile.value.temp_source?.temp_name,
)

const chosenFunction: Ref<Function> = ref(
    settingsStore.functions.find((f) => f.uid === currentProfile.value.function_uid)!,
)
const selectedTemp: Ref<number | undefined> = ref()
const selectedDuty: Ref<number | undefined> = ref()
const selectedPointIndex: Ref<number | undefined> = ref()
const selectedTempSourceTemp: Ref<number | undefined> = ref()

// THIS IS ALMOST A STRAIGHT COPY from ProfileView
//------------------------------------------------------------------------------------------------------------------------------------------
// User Control Graph

const defaultSymbolSize: number = deviceStore.getREMSize(1.0)
const defaultSymbolColor: string = colors.themeColors.bg_two
const selectedSymbolSize: number = deviceStore.getREMSize(1.25)
const selectedSymbolColor: string = colors.themeColors.accent
const graphTempMinLimit: number = 0
const graphTempMaxLimit: number = 150
const axisXTempMin: Ref<number> = ref(props.tempMin ?? currentProfile.value.temp_min ?? 0)
const axisXTempMax: Ref<number> = ref(props.tempMax ?? currentProfile.value.temp_max ?? 100)
const dutyMin: number = 0
const dutyMax: number = 100
let firstTimeChoosingTemp: boolean = true

interface PointData {
    value: [number, number]
    symbolSize: number
    itemStyle: {
        color: string
    }
}

const lineSpace = (
    startValue: number,
    stopValue: number,
    cardinality: number,
    precision: number,
): Array<number> => {
    const arr = []
    const step = (stopValue - startValue) / (cardinality - 1)
    for (let i = 0; i < cardinality; i++) {
        const value = startValue + step * i
        arr.push(deviceStore.round(value, precision))
    }
    return arr
}

const defaultDataValues = (): Array<PointData> => {
    const result: Array<PointData> = []
    if (selectedTempSource != null) {
        const profileLength =
            selectedTempSource.profileMinLength <= 5 && selectedTempSource.profileMaxLength >= 5
                ? 5
                : selectedTempSource.profileMaxLength
        const temps = lineSpace(
            Math.max(selectedTempSource.tempMin, axisXTempMin.value),
            Math.min(selectedTempSource.tempMax, axisXTempMax.value),
            profileLength,
            1,
        )
        const duties = lineSpace(dutyMin, dutyMax, profileLength, 0)
        for (const [index, temp] of temps.entries()) {
            result.push({
                value: [temp, duties[index]],
                symbolSize: defaultSymbolSize,
                itemStyle: {
                    color: defaultSymbolColor,
                },
            })
        }
    } else {
        for (let i = 0; i < 100; i = i + 25) {
            const value = 25 * i
            result.push({
                value: [value, value],
                symbolSize: defaultSymbolSize,
                itemStyle: {
                    color: defaultSymbolColor,
                },
            })
        }
    }
    return result
}

const data: Array<PointData> = []
const initSeriesData = () => {
    data.length = 0
    if (currentProfile.value.speed_profile.length > 1 && selectedTempSource != null) {
        for (const point of currentProfile.value.speed_profile) {
            data.push({
                value: [point[0], point[1]],
                symbolSize: defaultSymbolSize,
                itemStyle: {
                    color: defaultSymbolColor,
                },
            })
        }
    } else {
        data.push(...defaultDataValues())
    }
}
initSeriesData()

const markAreaData: [
    {
        xAxis: number
    }[],
    {
        xAxis: number
    }[],
] = [
    [{ xAxis: axisXTempMin.value }, { xAxis: axisXTempMin.value }],
    [{ xAxis: axisXTempMax.value }, { xAxis: axisXTempMax.value }],
]

const graphicData: GraphicComponentLooseOption[] = []

const tempLineData: [
    {
        value: number[]
    },
    {
        value: number[]
    },
] = [{ value: [] }, { value: [] }]

const setTempSourceTemp = (): void => {
    if (selectedTempSource == null) {
        return
    }
    const tempValue: string | undefined = deviceStore.currentDeviceStatus
        .get(selectedTempSource.deviceUID)
        ?.get(selectedTempSource.tempName)?.temp
    if (tempValue == null) {
        return
    }
    selectedTempSourceTemp.value = Number(tempValue)
}
setTempSourceTemp()

const option = {
    title: {
        show: false,
    },
    tooltip: {
        position: 'top',
        appendTo: 'body',
        triggerOn: 'none',
        borderWidth: 2,
        borderColor: colors.themeColors.border,
        backgroundColor: colors.themeColors.bg_two,
        textStyle: {
            color: colors.themeColors.text_color,
            fontSize: deviceStore.getREMSize(1.0),
        },
        padding: [0, 5, 1, 7],
        transitionDuration: 0.0,
        formatter: function (params: any) {
            return (
                params.data.value[0].toFixed(1) +
                t('common.tempUnit') +
                ' ' +
                params.data.value[1].toFixed(0) +
                t('common.percentUnit')
            )
        },
    },
    grid: {
        show: false,
        top: deviceStore.getREMSize(0.7),
        left: 0,
        right: deviceStore.getREMSize(1.2),
        bottom: 0,
        containLabel: true,
    },
    xAxis: {
        min: axisXTempMin.value,
        max: axisXTempMax.value,
        type: 'value',
        splitNumber: 10,
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.95),
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
        cursor: 'no-drop',
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.95),
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
    // @ts-ignore
    series: [
        {
            id: 'a',
            type: 'line',
            smooth: 0.0,
            symbol: 'circle',
            symbolSize: defaultSymbolSize,
            itemStyle: {
                color: colors.themeColors.bg_two,
                borderColor: colors.themeColors.accent,
                borderWidth: 2,
            },
            lineStyle: {
                color: colors.themeColors.accent,
                width: 6,
                type: 'solid',
                shadowColor: undefined,
                // size of the blur around the line:
                shadowBlur: 20,
            },
            emphasis: {
                disabled: true, // won't work anyway with our draggable graphics that lay on top
            },
            markArea: {
                silent: true,
                itemStyle: {
                    color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
                        {
                            offset: 0,
                            color: colors.convertColorToRGBA(colors.themeColors.red, 0.2),
                        },
                        {
                            offset: 1,
                            color: colors.convertColorToRGBA(colors.themeColors.red, 0.0),
                        },
                    ]),
                    // color: colors.themeColors.red,
                    // opacity: 0.1,
                },
                emphasis: {
                    disabled: true,
                },
                data: markAreaData,
                animation: true,
                animationDuration: 300,
                animationDurationUpdate: 100,
            },
            // This is for the symbols that don't have draggable graphics on top, aka the last point
            cursor: 'no-drop',
            data: data,
        },
        {
            id: 'tempLine',
            type: 'line',
            smooth: false,
            symbol: 'none',
            lineStyle: {
                color: colors.themeColors.accent,
                width: 1,
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
                    fontSize: deviceStore.getREMSize(1.0),
                    color: selectedTempSource?.color,
                    rotate: 90,
                    offset: [0, -2],
                    formatter: (params: any): string => {
                        if (params.value == null) return ''
                        return Number(params.value).toFixed(1) + '°'
                    },
                },
                data: [
                    {
                        coord: [selectedTempSourceTemp.value, 95],
                        value: selectedTempSourceTemp.value,
                    },
                ],
            },
            z: 1,
            silent: true,
        },
        {
            // this is used as a non-interactable line area style
            id: 'line-area',
            type: 'line',
            smooth: 0.0,
            symbol: 'none',
            lineStyle: {
                color: 'transparent',
                width: 0,
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
            silent: true,
            z: 0,
            data: data,
        },
    ],
    animation: true,
    animationDuration: 200,
    animationDurationUpdate: 200,
}

const setGraphData = () => {
    if (selectedTempSource == null) {
        return
    }
    if (firstTimeChoosingTemp) {
        initSeriesData()
        firstTimeChoosingTemp = false
    } else {
        // force points to all fit into the new limits:
        data[0].value[0] = Math.max(selectedTempSource!.tempMin, axisXTempMin.value)
        data[data.length - 1].value[0] = Math.min(selectedTempSource!.tempMax, axisXTempMax.value)
        for (let i = 1; i < data.length - 1; i++) {
            controlPointMotionForTempX(data[i].value[0], i)
        }
    }
    // set xAxis min and max to +/- 5 from new limits: (semi-zoom)
    if (
        currentProfile.value.temp_min == null &&
        selectedTempSource!.tempMin > axisXTempMin.value + 5
    ) {
        axisXTempMin.value = selectedTempSource!.tempMin - 5
        option.xAxis.min = selectedTempSource!.tempMin - 5
    } else if (
        currentProfile.value.temp_min == null &&
        currentProfile.value.speed_profile.length > 0
    ) {
        // No Axis range set, but speed profile exists: use current data range
        axisXTempMin.value = currentProfile.value.speed_profile[0][0]
        option.xAxis.min = currentProfile.value.speed_profile[0][0]
    } else if (currentProfile.value.temp_min != null) {
        // reset axis range:
        const minTemp = Math.max(currentProfile.value.temp_min, graphTempMinLimit)
        option.xAxis.min = minTemp
        axisXTempMin.value = minTemp
    }
    if (
        currentProfile.value.temp_max == null &&
        selectedTempSource!.tempMax < axisXTempMax.value - 5
    ) {
        const maxTemp = Math.min(selectedTempSource!.tempMax + 5, 100)
        axisXTempMax.value = maxTemp
        option.xAxis.max = maxTemp
    } else if (
        currentProfile.value.temp_max == null &&
        currentProfile.value.speed_profile.length > 0
    ) {
        // No Axis range set, but speed profile exists: use current data range
        const maxTemp = Math.min(
            currentProfile.value.speed_profile[currentProfile.value.speed_profile.length - 1][0] +
                5,
            100,
        )
        axisXTempMax.value = maxTemp
        option.xAxis.max = maxTemp
    } else if (currentProfile.value.temp_max != null) {
        // reset axis range:
        const maxTemp = Math.min(currentProfile.value.temp_max, graphTempMaxLimit)
        option.xAxis.max = maxTemp
        axisXTempMax.value = maxTemp
    }
    // set limited Mark Area
    markAreaData[0] = [{ xAxis: axisXTempMin.value }, { xAxis: selectedTempSource!.tempMin }]
    markAreaData[1] = [
        { xAxis: Math.min(selectedTempSource!.tempMax, 100) },
        { xAxis: axisXTempMax.value },
    ]
    setTempSourceTemp()
    // @ts-ignore
    option.series[1].lineStyle.color = selectedTempSource.color
    // @ts-ignore
    option.series[1].markPoint.label.color = selectedTempSource.color
    // @ts-ignore
    option.series[1].markPoint.data[0].coord[0] = selectedTempSourceTemp.value
    // @ts-ignore
    option.series[1].markPoint.data[0].value = selectedTempSourceTemp.value
    tempLineData[0].value = [selectedTempSourceTemp.value!, dutyMin]
    tempLineData[1].value = [selectedTempSourceTemp.value!, dutyMax]
}
const setFunctionGraphData = (): void => {
    if (chosenFunction.value.f_type === FunctionType.Identity) {
        option.series[0].smooth = 0.0
        option.series[2].smooth = 0.0
        // @ts-ignore
        option.series[0].lineStyle.shadowColor = colors.themeColors.bg_one
        option.series[0].lineStyle.shadowBlur = 10
    } else {
        option.series[0].smooth = 0.3
        option.series[2].smooth = 0.3
        // @ts-ignore
        option.series[0].lineStyle.shadowColor = colors.themeColors.accent
        // size of the blur around the line:
        option.series[0].lineStyle.shadowBlur = 20
    }
}
// if (selectedTempSource != null) {
//     // set chosenTemp on startup if set in profile
//     for (const availableTempSource of tempSources.value) {
//         if (availableTempSource.deviceUID !== selectedTempSource.deviceUID) {
//             continue
//         }
//         for (const availableTemp of availableTempSource.temps) {
//             if (
//                 availableTemp.deviceUID === selectedTempSource.deviceUID &&
//                 availableTemp.tempName === selectedTempSource.tempName
//             ) {
//                 chosenTemp.value = availableTemp
//                 break
//             }
//         }
//     }
// }
setGraphData()
setFunctionGraphData()

const updateTemps = () => {
    for (const tempDevice of tempSources.value) {
        for (const availableTemp of tempDevice.temps) {
            availableTemp.temp =
                currentDeviceStatus.value.get(availableTemp.deviceUID)!.get(availableTemp.tempName)!
                    .temp || '0.0'
        }
    }
}
watch(currentDeviceStatus, () => {
    updateTemps()
    if (selectedTempSource == null) {
        return
    }
    setTempSourceTemp()
    tempLineData[0].value = [selectedTempSourceTemp.value!, dutyMin]
    tempLineData[1].value = [selectedTempSourceTemp.value!, dutyMax]
    // there is a strange error only on the first time once switches back to a graph profile: Unknown series error
    controlGraph.value?.setOption({
        series: {
            id: 'tempLine',
            data: tempLineData,
            markPoint: {
                data: [
                    {
                        coord: [selectedTempSourceTemp.value!, 95],
                        value: selectedTempSourceTemp.value!,
                    },
                ],
            },
        },
    })
})

watch(settingsStore.allUIDeviceSettings, () => {
    // update all temp sources:
    fillTempSources()
    selectedTempSource = getCurrentTempSource(
        props.tempSource?.device_uid,
        props.tempSource?.temp_name,
    )
    if (selectedTempSource == null) {
        return
    }
    // @ts-ignore
    option.series[1].lineStyle.color = selectedTempSource.color
    controlGraph.value?.setOption({
        series: {
            id: 'tempLine',
            lineStyle: { color: selectedTempSource?.color },
            markPoint: { label: { color: selectedTempSource?.color } },
        },
    })
})

const controlPointMotionForTempX = (posX: number, selectedPointIndex: number): void => {
    // We use 1 whole degree of separation between points so point index works perfect:
    const minActivePosition =
        Math.max(selectedTempSource!.tempMin, axisXTempMin.value) + selectedPointIndex
    const maxActivePosition =
        Math.min(selectedTempSource!.tempMax, axisXTempMax.value) -
        (data.length - (selectedPointIndex + 1))
    if (selectedPointIndex === 0) {
        // starting point is horizontally fixed
        posX = minActivePosition
    } else if (selectedPointIndex === data.length - 1) {
        // final point is horizontally fixed
        posX = maxActivePosition
    }
    if (posX < minActivePosition) {
        posX = minActivePosition
    } else if (posX > maxActivePosition) {
        posX = maxActivePosition
    }
    data[selectedPointIndex].value[0] = posX
    // handle the points above the current point
    for (let i = selectedPointIndex + 1; i < data.length; i++) {
        const indexDiff = i - selectedPointIndex // index difference = degree difference
        const comparisonLimit = posX + indexDiff
        if (data[i].value[0] <= comparisonLimit) {
            data[i].value[0] = comparisonLimit
        }
    }
    // handle points below the current point
    for (let i = 0; i < selectedPointIndex; i++) {
        const indexDiff = selectedPointIndex - i
        const comparisonLimit = posX - indexDiff
        if (data[i].value[0] >= comparisonLimit) {
            data[i].value[0] = comparisonLimit
        }
    }
}

const controlPointMotionForDutyY = (posY: number, selectedPointIndex: number): void => {
    if (selectedPointIndex === data.length - 1) {
        return // last point is vertically fixed
    }
    if (posY < dutyMin) {
        posY = dutyMin
    } else if (posY > dutyMax) {
        posY = dutyMax
    }
    data[selectedPointIndex].value[1] = posY
    // handle the points above the current point
    for (let i = selectedPointIndex + 1; i < data.length; i++) {
        if (data[i].value[1] < posY) {
            data[i].value[1] = posY
        }
    }
    // handle points below the current point
    for (let i = 0; i < selectedPointIndex; i++) {
        if (data[i].value[1] > posY) {
            data[i].value[1] = posY
        }
    }
}

//----------------------------------------------------------------------------------------------------------------------
const controlGraph = ref<InstanceType<typeof VChart> | null>(null)

const setTempAndDutyValues = (dataIndex: number): void => {
    selectedTemp.value = deviceStore.round(data[dataIndex].value[0], 1)
    selectedDuty.value = deviceStore.round(data[dataIndex].value[1])
}

const onPointDragging = (dataIndex: number, posXY: [number, number]): void => {
    // Point dragging needs to be very fast and efficient. We'll set the points to their allowed positions on drag end
    data[dataIndex].value = posXY
    controlGraph.value?.setOption({
        series: [
            { id: 'a', data: data },
            { id: 'line-area', data: data },
        ],
    })
}

const afterPointDragging = (dataIndex: number, posXY: [number, number]): void => {
    // what needs to happen AFTER the dragging is done:
    controlPointMotionForTempX(posXY[0], dataIndex)
    controlPointMotionForDutyY(posXY[1], dataIndex)
    controlGraph.value?.setOption({
        series: [
            { id: 'a', data: data, markArea: { data: markAreaData } },
            { id: 'line-area', data: data },
        ],
        graphic: data
            .slice(0, data.length - 1) // no graphic for ending point
            .map((item, dataIndex) => ({
                id: dataIndex,
                type: 'circle',
                position: controlGraph.value?.convertToPixel('grid', item.value),
            })),
        xAxis: { min: axisXTempMin.value, max: axisXTempMax.value },
    })
}

const showTooltip = (dataIndex: number): void => {
    controlGraph.value?.dispatchAction({
        type: 'showTip',
        seriesIndex: 0,
        dataIndex: dataIndex,
    })
}

const hideTooltip = (): void => {
    controlGraph.value?.dispatchAction({
        type: 'hideTip',
    })
}

const createWatcherOfTempDutyText = (): WatchStopHandle =>
    watch(
        [selectedTemp, selectedDuty],
        (newTempAndDuty) => {
            if (selectedPointIndex.value == null) {
                return
            }
            controlPointMotionForTempX(newTempAndDuty[0]!, selectedPointIndex.value)
            controlPointMotionForDutyY(newTempAndDuty[1]!, selectedPointIndex.value)
            data.slice(0, data.length - 1) // no graphic for ending point
                .forEach(
                    (pointData, dataIndex) =>
                        // @ts-ignore
                        (graphicData[dataIndex].position = controlGraph.value?.convertToPixel(
                            'grid',
                            pointData.value,
                        )),
                )
            controlGraph.value?.setOption({
                series: [
                    { id: 'a', data: data },
                    { id: 'line-area', data: data },
                ],
                graphic: graphicData,
            })
        },
        { flush: 'post' },
    )
let tempDutyTextWatchStopper = createWatcherOfTempDutyText()

const createGraphicDataFromPointData = () => {
    const createGraphicDataForPoint = (dataIndex: number, posXY: [number, number]) => {
        return {
            id: dataIndex,
            type: 'circle',
            position: controlGraph.value?.convertToPixel('grid', posXY),
            shape: {
                cx: 0,
                cy: 0,
                r: selectedSymbolSize / 2 + 3, // a little extra space to make it easier to click
            },
            cursor: 'grab',
            silent: false,
            invisible: true,
            draggable: true,
            ondrag: function (eChartEvent: any) {
                if (eChartEvent?.event?.buttons !== 1) {
                    return // only apply on left button press
                }
                const posXY = (controlGraph.value?.convertFromPixel('grid', [
                    (this as any).x,
                    (this as any).y,
                ]) as [number, number]) ?? [0, 0]
                onPointDragging(dataIndex, posXY)
                showTooltip(dataIndex)
                this.cursor = 'grabbing'
            },
            onmouseup: function () {
                // We use 'onmouseup' instead of 'ondragend' here because onmouseup is only triggered in ECharts by the release
                // of the left mouse button, and ondragend is triggered by both left and right mouse buttons,
                // causing undesired behavior when deleting a selected point.
                // NOTE: the button number returned in both functions is 0 (none)
                const posXY = (controlGraph.value?.convertFromPixel('grid', [
                    (this as any).x,
                    (this as any).y,
                ]) as [number, number]) ?? [0, 0]
                afterPointDragging(dataIndex, posXY)
                setTempAndDutyValues(dataIndex)
                this.cursor = 'grab'
            },
            ondragend: function () {
                // the only real benefit of ondragend, is that it works even when the point has moved out of scope of the graph
                const [posX, posY] = (controlGraph.value?.convertFromPixel('grid', [
                    (this as any).x,
                    (this as any).y,
                ]) as [number, number]) ?? [0, 0]
                if (
                    posX < axisXTempMin.value ||
                    posX > axisXTempMax.value ||
                    posY < dutyMin ||
                    posY > dutyMax
                ) {
                    afterPointDragging(dataIndex, [posX, posY])
                    setTempAndDutyValues(dataIndex)
                    this.cursor = 'grab'
                }
            },
            onmouseover: function (eChartEvent: any) {
                if (eChartEvent?.event?.buttons !== 0) {
                    // EChart button numbers are different. 0=None, 1=Left, 2=Right
                    return // only react when no buttons are pressed (better drag UX)
                }
                tempDutyTextWatchStopper()
                setTempAndDutyValues(dataIndex)
                selectedPointIndex.value = dataIndex // sets the selected point on move over
                showTooltip(dataIndex)
                this.cursor = 'grab'
            },
            onmouseout: function (eChartEvent: any) {
                if (eChartEvent?.event?.buttons !== 0) {
                    return // only react when no buttons are pressed (better drag UX)
                }
                tempDutyTextWatchStopper() // make sure we stop and runny watchers before changing the reference
                tempDutyTextWatchStopper = createWatcherOfTempDutyText()
                hideTooltip()
            },
            z: 100,
        }
    }
    // clear and push
    graphicData.length = 0
    graphicData.push(
        ...data
            .slice(0, data.length - 1) // no graphic for ending point
            .map((item, dataIndex) => createGraphicDataForPoint(dataIndex, item.value)),
    )
}

const createDraggableGraphics = (): void => {
    // Add shadow circles (which is not visible) to enable drag.
    createGraphicDataFromPointData()
    controlGraph.value?.setOption({ graphic: graphicData })
}

const addPointToLine = (params: any) => {
    if (params.target?.type !== 'ec-polyline') {
        return
    }
    if (data.length >= selectedTempSource!.profileMaxLength) {
        //  (We'll have to convert the points ourselves to the proper points per device)
        return
    }
    selectedPointIndex.value = undefined
    const posXY = (controlGraph.value?.convertFromPixel('grid', [
        params.offsetX,
        params.offsetY,
    ]) as [number, number]) ?? [0, 0]
    // Clamp duty to min/max (with hi-res graphs, sometimes it went out of max bounds)
    posXY[1] = Math.min(Math.max(posXY[1], dutyMin), dutyMax)
    let indexToInsertAt = 1
    for (const [i, point] of data.entries()) {
        if (point.value[0] > posXY[0]) {
            indexToInsertAt = i
            break
        }
    }
    data.splice(indexToInsertAt, 0, {
        value: posXY,
        symbolSize: selectedSymbolSize,
        itemStyle: {
            color: selectedSymbolColor,
        },
    })
    // best to recreate all the graphics for this
    createGraphicDataFromPointData()
    // @ts-ignore
    option.series[0].data = data
    // @ts-ignore
    option.graphic = graphicData
    controlGraph.value?.setOption(option)
    // select the new point under the cursor:
    tempDutyTextWatchStopper()
    setTempAndDutyValues(indexToInsertAt)
    // this needs a bit of time for the graph to refresh before being set correctly:
    setTimeout(() => (selectedPointIndex.value = indexToInsertAt), 50)
    setTimeout(() => showTooltip(indexToInsertAt), 350) // wait until point animation is complete before showing tooltip
}

const deletePointFromLine = (params: any) => {
    if (params.componentType !== 'graphic' || params.event?.target?.id == null) {
        if (params.stop) {
            params.stop() // this stops any context menu from appearing in the graph
        }
        return
    }
    params.event.stop()
    if (data.length <= selectedTempSource!.profileMinLength) {
        return
    }
    const dataIndexToRemove: number = Number(params.event!.target!.id)
    if (dataIndexToRemove === 0 || dataIndexToRemove === data.length - 1) {
        return // we don't remove first or last points
    }
    data.splice(dataIndexToRemove, 1)
    // best to recreate all the graphics for this
    selectedPointIndex.value = undefined
    hideTooltip()
    createGraphicDataFromPointData()
    // needed to properly remove the graphic from the graph instance:
    // @ts-ignore
    option.series[0].data = data
    // @ts-ignore
    option.graphic = graphicData
    controlGraph.value?.setOption(option, { replaceMerge: ['series', 'graphic'], silent: true })
}

//--------------------------------------------------------------------------------------------------

// We show the graph always and right away
setTimeout(() => {
    // debounce because we need to wait for the graph to be rendered
    const resizeObserver = new ResizeObserver(
        _.debounce(
            () => {
                controlGraph.value?.setOption({
                    graphic: data.map(function (item, _dataIndex) {
                        return {
                            type: 'circle',
                            position: controlGraph.value?.convertToPixel('grid', item.value),
                        }
                    }),
                })
            },
            200,
            { leading: false },
        ),
    )
    resizeObserver.observe(controlGraph.value?.$el)
    createDraggableGraphics() // we need to create AFTER the element is visible and rendered
}, 500) // due to graph resizing, we really need a substantial delay on creation

const inputNumberTempMin = computed((): number => {
    if (selectedTempSource == null) {
        return axisXTempMin.value
    }
    // one degree of separation between points
    return (
        Math.max(selectedTempSource.tempMin, axisXTempMin.value) + (selectedPointIndex.value ?? 0)
    )
})

const inputNumberTempMax = computed((): number => {
    if (selectedTempSource == null) {
        return axisXTempMax.value
    }
    if (selectedPointIndex.value === 0) {
        return Math.max(selectedTempSource.tempMin, axisXTempMin.value) // starting point is horizontally fixed
    } else if (selectedPointIndex.value === data.length - 1) {
        return Math.min(selectedTempSource.tempMax, axisXTempMax.value) // last point is horizontally fixed
    }
    return (
        Math.min(selectedTempSource.tempMax, axisXTempMax.value) -
        (data.length - 1 - (selectedPointIndex.value ?? 0))
    )
})

const inputAxisMinNumberMin = computed((): number => {
    return graphTempMinLimit
})

const inputAxisMinNumberMax = computed((): number => {
    return Math.min(graphTempMaxLimit, axisXTempMax.value - 20)
})

const inputAxisMaxNumberMin = computed((): number => {
    return Math.max(graphTempMinLimit, axisXTempMin.value + 20)
})

const inputAxisMaxNumberMax = computed((): number => {
    return graphTempMaxLimit
})

const updateResponsiveGraphHeight = (): void => {
    const graphEl = document.getElementById('control-graph-wiz')
    const controlPanel = document.getElementById('control-panel')
    if (graphEl != null && controlPanel != null) {
        const panelHeight = controlPanel.getBoundingClientRect().height
        if (panelHeight > 56) {
            // 4rem
            graphEl.style.height = `max(calc(80vh - (${panelHeight}px + 4.5rem)), 20rem)`
        } else {
            graphEl.style.height = 'max(calc(80vh - 10rem), 20rem)'
        }
    }
}
const updatePosition = (): void => {
    controlGraph.value?.setOption({
        graphic: data.slice(0, data.length - 1).map((item, dataIndex) => ({
            id: dataIndex,
            position: controlGraph.value?.convertToPixel('grid', item.value),
        })),
    })
}

onMounted(async () => {
    // Make sure on selected Point change, that there is only one.
    watch(selectedPointIndex, (dataIndex) => {
        for (const [index, pointData] of data.entries()) {
            if (index === dataIndex) {
                pointData.symbolSize = selectedSymbolSize
                pointData.itemStyle.color = selectedSymbolColor
            } else {
                pointData.symbolSize = defaultSymbolSize
                pointData.itemStyle.color = defaultSymbolColor
            }
        }
        controlGraph.value?.setOption({
            series: [{ id: 'a', data: data }],
        })
    })
    window.addEventListener('resize', updateResponsiveGraphHeight)
    setTimeout(updateResponsiveGraphHeight)

    // handle the graphics on graph resize & zoom
    controlGraph.value?.chart?.on('dataZoom', updatePosition)
    window.addEventListener('resize', updatePosition)

    watch(axisXTempMin, (newValue: number) => {
        option.xAxis.min = newValue
        markAreaData[0] = [{ xAxis: newValue }, { xAxis: selectedTempSource!.tempMin }]
        afterPointDragging(0, [Math.max(selectedTempSource!.tempMin, newValue), data[0].value[1]])
        if (selectedPointIndex.value != null) {
            setTempAndDutyValues(selectedPointIndex.value)
        }
        // The calculation for the new graphic points is done before the Axis is updated,
        // so we need to update the graphic points after that here.
        // We could update the axis first, and then update the graphic points, but
        // it doesn't have as smooth an animation.
        controlGraph.value?.setOption({
            graphic: data
                .slice(0, data.length - 1) // no graphic for ending point
                .map((item, dataIndex) => ({
                    id: dataIndex,
                    type: 'circle',
                    position: controlGraph.value?.convertToPixel('grid', item.value),
                })),
        })
    })
    watch(axisXTempMax, (newValue: number) => {
        option.xAxis.max = newValue
        markAreaData[1] = [
            { xAxis: Math.min(selectedTempSource!.tempMax, 100) },
            { xAxis: newValue },
        ]
        afterPointDragging(data.length - 1, [
            Math.min(selectedTempSource!.tempMax, newValue),
            data[data.length - 1].value[1],
        ])
        if (selectedPointIndex.value != null) {
            setTempAndDutyValues(selectedPointIndex.value)
        }
        // The calculation for the new graphic points is done before the Axis is updated,
        // so we need to update the graphic points after that here.
        // We could update the axis first, and then update the graphic points, but
        // it doesn't have as smooth an animation.
        controlGraph.value?.setOption({
            graphic: data
                .slice(0, data.length - 1) // no graphic for ending point
                .map((item, dataIndex) => ({
                    id: dataIndex,
                    type: 'circle',
                    position: controlGraph.value?.convertToPixel('grid', item.value),
                })),
        })
    })
})
onUnmounted(() => {
    window.removeEventListener('resize', updateResponsiveGraphHeight)
    window.removeEventListener('resize', updatePosition)
})

const nextStep = () => {
    const speedProfile: Array<[number, number]> = []
    for (const pointData of data) {
        speedProfile.push(pointData.value)
    }
    emit('speedProfile', speedProfile)
    emit('tempMin', axisXTempMin.value)
    emit('tempMax', axisXTempMax.value)
    emit('nextStep', 10)
}
</script>

<template>
    <div id="control-panel" class="flex flex-col w-[87vw]">
        <div id="profile-display" class="bg-bg-one rounded-lg">
            <v-chart
                id="control-graph-wiz"
                class="p-3"
                ref="controlGraph"
                :option="option"
                :autoresize="true"
                :manual-update="true"
                @contextmenu="deletePointFromLine"
                @zr:click="addPointToLine"
                @zr:contextmenu="deletePointFromLine"
            />
        </div>
        <div class="flex flex-row justify-between mt-4">
            <InputNumber
                :placeholder="t('components.axisOptions.min')"
                v-model="axisXTempMin"
                mode="decimal"
                class="h-11"
                :suffix="` ${t('common.tempUnit')}`"
                showButtons
                :min="inputAxisMinNumberMin"
                :max="inputAxisMinNumberMax"
                :use-grouping="false"
                :step="5"
                button-layout="horizontal"
                :input-style="{ width: '5rem' }"
                :disabled="selectedTempSource == null"
                v-tooltip.top="t('views.profiles.minProfileTemp')"
            >
                <template #incrementicon>
                    <span class="pi pi-plus" />
                </template>
                <template #decrementicon>
                    <span class="pi pi-minus" />
                </template>
            </InputNumber>
            <div class="flex flex-row">
                <InputNumber
                    :placeholder="t('common.duty')"
                    v-model="selectedDuty"
                    inputId="selected-duty"
                    mode="decimal"
                    class="duty-input h-11"
                    :suffix="` ${t('common.percentUnit')}`"
                    showButtons
                    :min="dutyMin"
                    :max="dutyMax"
                    :disabled="selectedPointIndex == null"
                    :use-grouping="false"
                    :step="1"
                    button-layout="horizontal"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.top="t('views.profiles.selectedPointDuty')"
                >
                    <template #incrementicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
                <div
                    class="p-2 mx-4 leading-none items-center"
                    v-tooltip.top="t('views.profiles.graphProfileMouseActions')"
                >
                    <svg-icon
                        type="mdi"
                        class="h-7"
                        :path="mdiInformationSlabCircleOutline"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                </div>
                <InputNumber
                    :placeholder="t('common.temperature')"
                    v-model="selectedTemp"
                    inputId="selected-temp"
                    mode="decimal"
                    class="temp-input h-11"
                    :suffix="` ${t('common.tempUnit')}`"
                    showButtons
                    :min="inputNumberTempMin"
                    :max="inputNumberTempMax"
                    :disabled="selectedPointIndex == null"
                    :use-grouping="false"
                    :step="0.1"
                    :min-fraction-digits="1"
                    :max-fraction-digits="1"
                    button-layout="horizontal"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.top="t('views.profiles.selectedPointTemp')"
                >
                    <template #incrementicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
            </div>
            <InputNumber
                :placeholder="t('components.axisOptions.max')"
                v-model="axisXTempMax"
                mode="decimal"
                class="h-11"
                :suffix="` ${t('common.tempUnit')}`"
                showButtons
                :min="inputAxisMaxNumberMin"
                :max="inputAxisMaxNumberMax"
                :use-grouping="false"
                :step="5"
                button-layout="horizontal"
                :input-style="{ width: '5rem' }"
                :disabled="selectedTempSource == null"
                v-tooltip.top="t('views.profiles.maxProfileTemp')"
            >
                <template #incrementicon>
                    <span class="pi pi-plus" />
                </template>
                <template #decrementicon>
                    <span class="pi pi-minus" />
                </template>
            </InputNumber>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 8)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button class="w-24 bg-bg-one" :label="t('common.next')" @click="nextStep" />
        </div>
    </div>
</template>

<style scoped lang="scss">
#control-graph-wiz #control-panel {
    overflow: hidden;
    // This is adjusted dynamically on resize with js above
    height: max(calc(80vh - 6rem), 20rem);
}
</style>
