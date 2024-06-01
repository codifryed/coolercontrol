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
import { useSettingsStore } from '@/stores/SettingsStore'
import {
    Function,
    ProfileMixFunctionType,
    Profile,
    ProfileTempSource,
    ProfileType,
} from '@/models/Profile'
import Button from 'primevue/button'
import Dropdown from 'primevue/dropdown'
import MultiSelect from 'primevue/multiselect'
import {
    computed,
    inject,
    onMounted,
    type Ref,
    ref,
    watch,
    type WatchStopHandle,
    nextTick,
} from 'vue'
import InputText from 'primevue/inputtext'
import InputNumber from 'primevue/inputnumber'
import Knob from 'primevue/knob'
import { useDeviceStore } from '@/stores/DeviceStore'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiChip } from '@mdi/js'
import * as echarts from 'echarts/core'
import {
    DataZoomComponent,
    GraphicComponent,
    GridComponent,
    MarkAreaComponent,
    MarkPointComponent,
    TooltipComponent,
} from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { UniversalTransition } from 'echarts/features'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import type { EChartsOption } from 'echarts'
import type { GraphicComponentLooseOption } from 'echarts/types/dist/shared.d.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { storeToRefs } from 'pinia'
import { useToast } from 'primevue/usetoast'
import { $enum } from 'ts-enum-util'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import MixProfileEditorChart from '@/components/MixProfileEditorChart.vue'

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
])

interface Props {
    profileUID: string
}

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const props: Props = dialogRef.value.data

const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const toast = useToast()

const currentProfile = computed(
    () => settingsStore.profiles.find((profile) => profile.uid === props.profileUID)!,
)
const givenName: Ref<string> = ref(currentProfile.value.name)
const selectedType: Ref<ProfileType> = ref(currentProfile.value.p_type)
const profileTypes = [...$enum(ProfileType).keys()]
const mixFunctionTypes = [...$enum(ProfileMixFunctionType).keys()]
const tempSourceInvalid: Ref<boolean> = ref(false)

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

const tempSources: Ref<Array<AvailableTempSources>> = ref([])
const fillTempSources = () => {
    tempSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.status.temps.length === 0 || device.info == null) {
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
            if (deviceSettings.sensorsAndChannels.get(temp.name)!.hide) {
                continue
            }
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

const chosenTemp: Ref<AvailableTemp | undefined> = ref()
const chosenFunction: Ref<Function> = ref(
    settingsStore.functions.find((f) => f.uid === currentProfile.value.function_uid)!,
)
const memberProfileOptions: Ref<Array<Profile>> = computed(() =>
    settingsStore.profiles.filter(
        (profile) => profile.uid !== props.profileUID && profile.p_type === ProfileType.Graph,
    ),
)
const chosenMemberProfiles: Ref<Array<Profile>> = ref(
    currentProfile.value.member_profile_uids.map(
        (uid) => settingsStore.profiles.find((profile) => profile.uid === uid)!,
    ),
)
const chosenProfileMixFunction: Ref<ProfileMixFunctionType> = ref(
    currentProfile.value.mix_function_type != null
        ? currentProfile.value.mix_function_type
        : ProfileMixFunctionType.Max,
)
const selectedTemp: Ref<number | undefined> = ref()
const selectedDuty: Ref<number | undefined> = ref()
const selectedPointIndex: Ref<number | undefined> = ref()
const selectedTempSourceTemp: Ref<number | undefined> = ref()

//------------------------------------------------------------------------------------------------------------------------------------------
// User Control Graph

const defaultSymbolSize: number = deviceStore.getREMSize(0.9)
const defaultSymbolColor: string = colors.themeColors.bg_three
const selectedSymbolSize: number = deviceStore.getREMSize(1.125)
const selectedSymbolColor: string = colors.themeColors.accent
const axisXTempMin: number = 0
const axisXTempMax: number = 100
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
            selectedTempSource.tempMin,
            selectedTempSource.tempMax,
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
    [{ xAxis: axisXTempMin }, { xAxis: axisXTempMin }],
    [{ xAxis: axisXTempMax }, { xAxis: axisXTempMax }],
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

const option: EChartsOption = {
    tooltip: {
        position: 'top',
        appendTo: 'body',
        triggerOn: 'none',
        borderWidth: 1,
        borderColor: colors.themeColors.text_color_secondary + 'FF',
        backgroundColor: colors.themeColors.bg_two + 'F0',
        textStyle: {
            color: colors.themeColors.accent,
            fontSize: deviceStore.getREMSize(0.9),
        },
        padding: [0, 3, 0, 3],
        transitionDuration: 0.3,
        formatter: function (params: any) {
            return params.data.value[1].toFixed(0) + '% ' + params.data.value[0].toFixed(1) + '°'
        },
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
        type: 'value',
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.9),
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
                color: colors.themeColors.gray_600,
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
            smooth: false,
            symbol: 'circle',
            symbolSize: defaultSymbolSize,
            itemStyle: {
                color: colors.themeColors.bg_three,
                borderColor: colors.themeColors.accent,
                borderWidth: 2,
            },
            lineStyle: {
                color: colors.themeColors.accent,
                width: 2,
                type: 'solid',
            },
            emphasis: {
                disabled: true, // won't work anyway with our draggable graphics that lay on top
            },
            markArea: {
                silent: true,
                itemStyle: {
                    color: colors.themeColors.red,
                    opacity: 0.1,
                },
                emphasis: {
                    disabled: true,
                },
                data: markAreaData,
                animation: true,
                animationDuration: 500,
                animationDurationUpdate: 300,
            },
            data: data,
        },
        {
            id: 'tempLine',
            type: 'line',
            smooth: false,
            symbol: 'none',
            lineStyle: {
                color: colors.themeColors.yellow,
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
                    fontSize: deviceStore.getREMSize(0.9),
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
    ],
    animation: true,
    animationDuration: 300,
    animationDurationUpdate: 300,
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
        data[0].value[0] = selectedTempSource!.tempMin
        data[data.length - 1].value[0] = selectedTempSource!.tempMax
        for (let i = 1; i < data.length - 1; i++) {
            controlPointMotionForTempX(data[i].value[0], i)
        }
    }
    markAreaData[0] = [{ xAxis: axisXTempMin }, { xAxis: selectedTempSource!.tempMin }]
    markAreaData[1] = [{ xAxis: selectedTempSource!.tempMax }, { xAxis: axisXTempMax }]
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
if (selectedTempSource != null) {
    // set chosenTemp on startup if set in profile
    for (const availableTempSource of tempSources.value) {
        if (availableTempSource.deviceUID !== selectedTempSource.deviceUID) {
            continue
        }
        for (const availableTemp of availableTempSource.temps) {
            if (
                availableTemp.deviceUID === selectedTempSource.deviceUID &&
                availableTemp.tempName === selectedTempSource.tempName
            ) {
                chosenTemp.value = availableTemp
                break
            }
        }
    }
    setGraphData()
}

const updateTemps = () => {
    for (const tempDevice of tempSources.value) {
        for (const availableTemp of tempDevice.temps) {
            availableTemp.temp =
                currentDeviceStatus.value.get(availableTemp.deviceUID)!.get(availableTemp.tempName)!
                    .temp || '0.0'
        }
    }
}

watch(chosenTemp, () => {
    selectedTempSource = getCurrentTempSource(
        chosenTemp.value?.deviceUID,
        chosenTemp.value?.tempName,
    )
    setGraphData()
    controlGraph.value?.setOption(option)
})

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
        chosenTemp.value?.deviceUID,
        chosenTemp.value?.tempName,
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
    const minActivePosition = selectedTempSource!.tempMin + selectedPointIndex
    if (selectedPointIndex === 0) {
        data[selectedPointIndex].value[0] = minActivePosition
        return // starting point is horizontally fixed
    } else if (selectedPointIndex === data.length - 1) {
        return // last point is horizontally fixed
    }
    const maxActivePosition = selectedTempSource!.tempMax - (data.length - (selectedPointIndex + 1))
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
        series: [{ id: 'a', data: data }],
    })
}

const afterPointDragging = (dataIndex: number, posXY: [number, number]): void => {
    // what needs to happen AFTER the dragging is done:
    controlPointMotionForTempX(posXY[0], dataIndex)
    controlPointMotionForDutyY(posXY[1], dataIndex)
    controlGraph.value?.setOption({
        series: [{ id: 'a', data: data }],
        graphic: data
            .slice(0, data.length - 1) // no graphic for ending point
            .map((item, dataIndex) => ({
                id: dataIndex,
                type: 'circle',
                position: controlGraph.value?.convertToPixel('grid', item.value),
            })),
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
                series: [{ id: 'a', data: data }],
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
                r: selectedSymbolSize / 2,
            },
            cursor: 'pointer',
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
            },
            ondragend: function () {
                // the only real benefit of ondragend, is that it works even when the point has moved out of scope of the graph
                const [posX, posY] = (controlGraph.value?.convertFromPixel('grid', [
                    (this as any).x,
                    (this as any).y,
                ]) as [number, number]) ?? [0, 0]
                if (
                    posX < axisXTempMin ||
                    posX > axisXTempMax ||
                    posY < dutyMin ||
                    posY > dutyMax
                ) {
                    afterPointDragging(dataIndex, [posX, posY])
                    setTempAndDutyValues(dataIndex)
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

let draggableGraphicsCreated: boolean = false
const createDraggableGraphics = (): void => {
    // Add shadow circles (which is not visible) to enable drag.
    if (draggableGraphicsCreated) {
        return // we only need to do this once, AFTER the graph is drawn and visible
    }
    createGraphicDataFromPointData()
    controlGraph.value?.setOption({ graphic: graphicData })
    draggableGraphicsCreated = true
}

const addPointToLine = (params: any) => {
    if (params.target?.type !== 'ec-polyline') {
        return
    }
    if (data.length >= selectedTempSource!.profileMaxLength) {
        // todo: actually profile length belongs to the channel/duty device being set.
        //  (We'll have to convert the points ourselves to the proper points per device)
        return
    }
    selectedPointIndex.value = undefined
    const posXY = (controlGraph.value?.convertFromPixel('grid', [
        params.offsetX,
        params.offsetY,
    ]) as [number, number]) ?? [0, 0]
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
        params.stop() // this stops any context menu from appearing in the graph, even though it sometimes throws an error
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

const showGraph = computed(() => {
    const shouldShow =
        selectedType.value != null &&
        selectedType.value === ProfileType.Graph &&
        chosenTemp.value != null
    if (shouldShow) {
        setTimeout(() => {
            const resizeObserver = new ResizeObserver((_) => {
                controlGraph.value?.setOption({
                    graphic: data.map(function (item, _dataIndex) {
                        return {
                            type: 'circle',
                            position: controlGraph.value?.convertToPixel('grid', item.value),
                        }
                    }),
                })
            })
            resizeObserver.observe(controlGraph.value?.$el)
            createDraggableGraphics() // we need to create AFTER the element is visible and rendered
        }, 500) // due to graph resizing, we really need a substantial delay on creation
    }
    return shouldShow
})

const showDutyKnob = computed(() => {
    const shouldShow = selectedType.value != null && selectedType.value === ProfileType.Fixed
    if (shouldShow) {
        selectedDuty.value = currentProfile.value.speed_fixed ?? 50 // reasonable default if not already set
        selectedPointIndex.value = undefined // clear previous selected graph point
    }
    return shouldShow
})

const showMixChart = computed(
    () => selectedType.value != null && selectedType.value === ProfileType.Mix,
)
const mixProfileKeys: Ref<string> = computed(() =>
    chosenMemberProfiles.value.map((p) => p.uid).join(':'),
)

const inputNumberTempMin = () => {
    if (selectedTempSource == null) {
        return axisXTempMin
    }
    return selectedTempSource.tempMin + (selectedPointIndex.value ?? 0)
}

const inputNumberTempMax = () => {
    if (selectedTempSource == null) {
        return axisXTempMax
    }
    return selectedTempSource.tempMax - (data.length - 1 - (selectedPointIndex.value ?? 0))
}

const editFunctionEnabled = () => {
    return currentProfile.value.uid !== '0' && chosenFunction.value.uid !== '0'
}
const goToFunction = (): void => {
    dialogRef.value.close({ functionUID: chosenFunction.value.uid })
}

const saveProfileState = async () => {
    currentProfile.value.name = givenName.value
    currentProfile.value.p_type = selectedType.value
    if (currentProfile.value.p_type === ProfileType.Fixed) {
        currentProfile.value.speed_fixed = selectedDuty.value
        currentProfile.value.speed_profile.length = 0
        currentProfile.value.temp_source = undefined
        currentProfile.value.function_uid = '0' // default function
        currentProfile.value.member_profile_uids.length = 0
        currentProfile.value.mix_function_type = undefined
    } else if (currentProfile.value.p_type === ProfileType.Graph) {
        if (selectedTempSource === undefined) {
            tempSourceInvalid.value = true
            toast.add({
                severity: 'error',
                summary: 'Error',
                detail: 'A Temp Source is required for Graph Profiles',
                life: 3000,
            })
            return
        } else {
            tempSourceInvalid.value = false
        }
        const speedProfile: Array<[number, number]> = []
        for (const pointData of data) {
            speedProfile.push(pointData.value)
        }
        currentProfile.value.speed_profile = speedProfile
        currentProfile.value.temp_source = new ProfileTempSource(
            selectedTempSource.tempName,
            selectedTempSource.deviceUID,
        )
        currentProfile.value.function_uid = chosenFunction.value.uid
        currentProfile.value.speed_fixed = undefined
        currentProfile.value.member_profile_uids.length = 0
        currentProfile.value.mix_function_type = undefined
    } else if (currentProfile.value.p_type === ProfileType.Mix) {
        currentProfile.value.speed_fixed = undefined
        currentProfile.value.speed_profile.length = 0
        currentProfile.value.temp_source = undefined
        currentProfile.value.function_uid = '0' // default function
        currentProfile.value.member_profile_uids = chosenMemberProfiles.value.map((p) => p.uid)
        currentProfile.value.mix_function_type = chosenProfileMixFunction.value
    }
    const successful = await settingsStore.updateProfile(currentProfile.value.uid)
    if (successful) {
        toast.add({
            severity: 'success',
            summary: 'Success',
            detail: 'Profile successfully updated and applied to affected devices',
            life: 3000,
        })
        dialogRef.value.close()
    } else {
        toast.add({
            severity: 'error',
            summary: 'Error',
            detail: 'There was an error attempting to update this Profile',
            life: 3000,
        })
    }
}

const tempScrolled = (event: WheelEvent): void => {
    if (selectedTemp.value == null) return
    if (event.deltaY < 0) {
        if (selectedTemp.value < inputNumberTempMax()) selectedTemp.value += 1
    } else {
        if (selectedTemp.value > inputNumberTempMin()) selectedTemp.value -= 1
    }
}
const dutyScrolled = (event: WheelEvent): void => {
    if (selectedDuty.value == null) return
    if (event.deltaY < 0) {
        if (selectedDuty.value < dutyMax) selectedDuty.value += 1
    } else {
        if (selectedDuty.value > dutyMin) selectedDuty.value -= 1
    }
}
//----------------------------------------------------------------------------------------------------------------------

const applyButton = ref()
nextTick(async () => {
    const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
    await delay()
    applyButton.value.$el.focus()
})

const addScrollEventListeners = (): void => {
    // @ts-ignore
    document?.querySelector('.temp-input')?.addEventListener('wheel', tempScrolled)
    // @ts-ignore
    document?.querySelector('.duty-input')?.addEventListener('wheel', dutyScrolled)
    // @ts-ignore
    document?.querySelector('.duty-knob-input')?.addEventListener('wheel', dutyScrolled)
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
    // handle the graphics on graph resize & zoom
    const updatePosition = (): void => {
        controlGraph.value?.setOption({
            graphic: data.slice(0, data.length - 1).map((item, dataIndex) => ({
                id: dataIndex,
                position: controlGraph.value?.convertToPixel('grid', item.value),
            })),
        })
    }
    controlGraph.value?.chart?.on('dataZoom', updatePosition)
    window.addEventListener('resize', updatePosition)
    addScrollEventListeners()
    // re-add some scroll event listeners for elements that are rendered on Type change
    watch(selectedType, () => {
        nextTick(addScrollEventListeners)
    })
})
</script>

<template>
    <div class="grid grid-webkit-fix">
        <div class="col-fixed" style="width: 16rem">
            <span class="p-float-label mt-4">
                <InputText id="name" v-model="givenName" class="w-full" />
                <label for="name">Name</label>
            </span>
            <div class="p-float-label mt-4">
                <Dropdown
                    v-model="selectedType"
                    inputId="dd-profile-type"
                    :options="profileTypes"
                    placeholder="Type"
                    class="w-full"
                    scroll-height="400px"
                />
                <label for="dd-profile-type">Type</label>
            </div>
            <div v-if="selectedType === ProfileType.Graph" class="p-float-label mt-4">
                <Dropdown
                    v-model="chosenTemp"
                    inputId="dd-temp-source"
                    :options="tempSources"
                    option-label="tempFrontendName"
                    option-group-label="deviceName"
                    option-group-children="temps"
                    placeholder="Temp Source"
                    :class="['w-full', { 'p-invalid': tempSourceInvalid }]"
                    scroll-height="400px"
                >
                    <template #optiongroup="slotProps">
                        <div class="flex align-items-center">
                            <svg-icon
                                type="mdi"
                                :path="mdiChip"
                                :size="deviceStore.getREMSize(1.3)"
                                class="mr-2"
                            />
                            <div>{{ slotProps.option.deviceName }}</div>
                        </div>
                    </template>
                    <template #option="slotProps">
                        <div class="flex align-items-center justify-content-between">
                            <div>
                                <span
                                    class="pi pi-minus mr-2 ml-1"
                                    :style="{ color: slotProps.option.lineColor }"
                                />{{ slotProps.option.tempFrontendName }}
                            </div>
                            <div>
                                {{ slotProps.option.temp + ' °' }}
                            </div>
                        </div>
                    </template>
                </Dropdown>
                <label for="dd-temp-source">Temp Source</label>
            </div>
            <div v-if="selectedType === ProfileType.Graph" class="p-float-label mt-4">
                <Dropdown
                    v-model="chosenFunction"
                    inputId="dd-function"
                    :options="settingsStore.functions"
                    option-label="name"
                    placeholder="Function"
                    class="w-full"
                    scroll-height="400px"
                />
                <label for="dd-function">Function</label>
            </div>
            <div v-if="selectedType === ProfileType.Mix" class="p-float-label mt-4">
                <MultiSelect
                    v-model="chosenMemberProfiles"
                    inputId="dd-member-profiles"
                    :options="memberProfileOptions"
                    option-label="name"
                    placeholder="Member Profiles"
                    :class="['w-full']"
                    scroll-height="400px"
                >
                    <template #option="slotProps">
                        <div>
                            {{ slotProps.option.name }}
                        </div>
                    </template>
                </MultiSelect>
                <label for="dd-member-profiles">Member Profiles</label>
            </div>
            <div v-if="selectedType === ProfileType.Mix" class="p-float-label mt-4">
                <Dropdown
                    v-model="chosenProfileMixFunction"
                    inputId="dd-mix-function"
                    :options="mixFunctionTypes"
                    placeholder="Mix Function"
                    class="w-full"
                    scroll-height="400px"
                />
                <label for="dd-mix-function">Mix Function</label>
            </div>
            <div class="align-content-end">
                <div
                    v-if="selectedType === ProfileType.Fixed || selectedType === ProfileType.Graph"
                    class="mt-6"
                >
                    <div v-if="selectedType === ProfileType.Graph" class="selected-point-wrapper">
                        <label for="selected-point">For Selected Point:</label>
                    </div>
                    <InputNumber
                        placeholder="Duty"
                        v-model="selectedDuty"
                        inputId="selected-duty"
                        mode="decimal"
                        class="duty-input w-full"
                        suffix="%"
                        :input-style="{ width: '58px' }"
                        showButtons
                        :min="dutyMin"
                        :max="dutyMax"
                        :disabled="selectedPointIndex == null && !showDutyKnob"
                    />
                </div>
                <div v-if="selectedType === ProfileType.Graph" class="mt-3">
                    <InputNumber
                        placeholder="Temp"
                        v-model="selectedTemp"
                        inputId="selected-temp"
                        mode="decimal"
                        suffix="°"
                        showButtons
                        class="temp-input w-full"
                        :disabled="!selectedPointIndex"
                        :min="inputNumberTempMin()"
                        :max="inputNumberTempMax()"
                        buttonLayout="horizontal"
                        :step="0.1"
                        :input-style="{ width: '55px' }"
                        incrementButtonIcon="pi pi-angle-right"
                        decrementButtonIcon="pi pi-angle-left"
                    />
                </div>
                <Button
                    v-if="selectedType === ProfileType.Graph"
                    label="Edit Function"
                    class="mt-6 w-full"
                    outlined
                    :disabled="!editFunctionEnabled()"
                    @click="goToFunction"
                >
                    <span class="p-button-label">Edit Function</span>
                </Button>
                <div class="mt-5">
                    <Button
                        ref="applyButton"
                        label="Apply"
                        class="w-full"
                        @click="saveProfileState"
                    >
                        <span class="p-button-label">Apply</span>
                    </Button>
                </div>
            </div>
        </div>
        <!-- The UI Display: -->
        <div class="col pb-0">
            <v-chart
                v-show="showGraph"
                class="control-graph pr-3"
                ref="controlGraph"
                :option="option"
                :autoresize="true"
                :manual-update="true"
                @contextmenu="deletePointFromLine"
                @zr:click="addPointToLine"
                @zr:contextmenu="deletePointFromLine"
            />
            <Knob
                v-show="showDutyKnob"
                v-model="selectedDuty"
                valueTemplate="{value}%"
                :min="dutyMin"
                :max="dutyMax"
                :step="1"
                :size="deviceStore.getREMSize(20)"
                class="duty-knob-input text-center mt-3"
            />
            <MixProfileEditorChart
                v-show="showMixChart"
                :profiles="chosenMemberProfiles"
                :mixFunctionType="chosenProfileMixFunction"
                :key="mixProfileKeys"
                class="mt-3"
            />
        </div>
    </div>
</template>

<style scoped lang="scss">
.control-graph {
    height: max(70vh, 40rem);
    width: max(calc(90vw - 17rem), 20rem);
}

.fade-enter-active,
.fade-leave-active {
    transition: all 0.5s ease;
}

.fade-enter-from,
.fade-leave-to {
    opacity: 0;
}

.selected-point-wrapper {
    margin-left: 0.75rem;
    margin-bottom: 0.25rem;
    padding: 0;
    font-size: 0.75rem;
    color: var(--text-color-secondary);
}

// This is needed particularly in Tauri, as it moves to multiline flex-wrap as soon as the scrollbar
//  appears. Other browsers don't do this, so we need to force it to nowrap.
.grid-webkit-fix {
    @media screen and (min-width: 38rem) {
        -webkit-flex-wrap: nowrap;
    }
}
</style>
