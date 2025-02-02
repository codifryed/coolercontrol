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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiContentSaveOutline, mdiInformationSlabCircleOutline, mdiMemory } from '@mdi/js'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import {
    Function,
    Profile,
    ProfileMixFunctionType,
    ProfileTempSource,
    ProfileType,
} from '@/models/Profile.ts'
import Button from 'primevue/button'
import MultiSelect from 'primevue/multiselect'
import { computed, nextTick, onMounted, ref, type Ref, watch, type WatchStopHandle } from 'vue'
import InputNumber from 'primevue/inputnumber'
import Knob from 'primevue/knob'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
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
import type { GraphicComponentLooseOption } from 'echarts/types/dist/shared.d.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { storeToRefs } from 'pinia'
import { useToast } from 'primevue/usetoast'
import { $enum } from 'ts-enum-util'
import MixProfileEditorChart from '@/components/MixProfileEditorChart.vue'
import Select from 'primevue/select'
import { onBeforeRouteLeave, onBeforeRouteUpdate } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'
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
])

interface Props {
    profileUID: string
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const toast = useToast()
const confirm = useConfirm()

let contextIsDirty: boolean = false

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

const defaultSymbolSize: number = deviceStore.getREMSize(1.0)
const defaultSymbolColor: string = colors.themeColors.bg_two
const selectedSymbolSize: number = deviceStore.getREMSize(1.25)
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

const option = {
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
            return params.data.value[1].toFixed(0) + '% ' + params.data.value[0].toFixed(1) + '°'
        },
    },
    grid: {
        show: false,
        top: deviceStore.getREMSize(0.7),
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
        cursor: 'no-drop',
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
    // @ts-ignore
    series: [
        {
            id: 'a',
            type: 'line',
            smooth: false,
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
                    posX < axisXTempMin ||
                    posX > axisXTempMax ||
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

// todo: delete later - looks like with the v-if logic, we no longer need the boolean
// let draggableGraphicsCreated: boolean = false
const createDraggableGraphics = (): void => {
    // Add shadow circles (which is not visible) to enable drag.
    // if (draggableGraphicsCreated) {
    //     return // we only need to do this once, AFTER the graph is drawn and visible
    // }
    createGraphicDataFromPointData()
    controlGraph.value?.setOption({ graphic: graphicData })
    // draggableGraphicsCreated = true
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

const inputNumberTempMin = (): number => {
    if (selectedTempSource == null) {
        return axisXTempMin
    }
    return selectedTempSource.tempMin + (selectedPointIndex.value ?? 0)
}

const inputNumberTempMax = (): number => {
    if (selectedTempSource == null) {
        return axisXTempMax
    }
    if (selectedPointIndex.value === 0) {
        return selectedTempSource.tempMin // starting point is horizontally fixed
    } else if (selectedPointIndex.value === data.length - 1) {
        return selectedTempSource.tempMax // last point is horizontally fixed
    }
    return selectedTempSource.tempMax - (data.length - 1 - (selectedPointIndex.value ?? 0))
}

// const editFunctionEnabled = () => {
//     return currentProfile.value.uid !== '0' && chosenFunction.value.uid !== '0'
// }
// const goToFunction = (): void => {
//     // router.push({ name: 'function', params: { functionUID: chosenFunction.value.uid } })
//     // dialogRef.value.close({ functionUID: chosenFunction.value.uid })
// }

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
        contextIsDirty = false
        toast.add({
            severity: 'success',
            summary: 'Success',
            detail: 'Profile successfully updated and applied to affected devices',
            life: 3000,
        })
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

const addScrollEventListeners = (): void => {
    // @ts-ignore
    document?.querySelector('.temp-input')?.addEventListener('wheel', tempScrolled)
    // @ts-ignore
    document?.querySelector('.duty-input')?.addEventListener('wheel', dutyScrolled)
    // @ts-ignore
    document?.querySelector('.duty-knob-input')?.addEventListener('wheel', dutyScrolled)
}
const contextIsVerifiedClean = (): boolean => {
    // For Profiles, we need deep checks to see what's changed, if anything.
    if (currentProfile.value.p_type === ProfileType.Fixed) {
        return currentProfile.value.speed_fixed === selectedDuty.value
    } else if (currentProfile.value.p_type === ProfileType.Graph) {
        if (selectedTempSource === undefined) {
            return false
        }
        const speedProfile: Array<[number, number]> = []
        for (const pointData of data) {
            speedProfile.push(pointData.value)
        }
        return (
            _.isEqual(currentProfile.value.speed_profile, speedProfile) &&
            currentProfile.value.temp_source?.temp_name === selectedTempSource.tempName &&
            currentProfile.value.temp_source?.device_uid === selectedTempSource.deviceUID &&
            currentProfile.value.function_uid === chosenFunction.value.uid
        )
    } else if (currentProfile.value.p_type === ProfileType.Mix) {
        return (
            _.isEqual(
                currentProfile.value.member_profile_uids,
                chosenMemberProfiles.value.map((p) => p.uid),
            ) && currentProfile.value.mix_function_type === chosenProfileMixFunction.value
        )
    }
    return true
}
const checkForUnsavedChanges = (_to: any, _from: any, next: any): void => {
    if (!contextIsDirty && contextIsVerifiedClean()) {
        next()
        return
    }
    confirm.require({
        message: 'There are unsaved changes made to this Profile.',
        header: 'Unsaved Changes',
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        rejectLabel: 'Stay',
        acceptLabel: 'Discard',
        accept: () => {
            next()
            contextIsDirty = false
        },
        reject: () => next(false),
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

    watch([chosenMemberProfiles, chosenTemp, chosenFunction, chosenProfileMixFunction], () => {
        contextIsDirty = true
    })
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="flex pl-4 py-2 text-2xl overflow-hidden">
            <span class="font-bold overflow-hidden overflow-ellipsis">{{
                currentProfile.name
            }}</span>
        </div>
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div v-if="selectedType === ProfileType.Mix" class="p-2 pr-0 flex flex-row">
                <Select
                    v-model="chosenProfileMixFunction"
                    :options="mixFunctionTypes"
                    placeholder="Mix Function"
                    class="w-20 mr-3"
                    checkmark
                    dropdown-icon="pi pi-sliders-v"
                    scroll-height="40rem"
                    v-tooltip.bottom="'Apply a Mix Function to the selected Profiles.'"
                />
                <MultiSelect
                    v-model="chosenMemberProfiles"
                    :options="memberProfileOptions"
                    option-label="name"
                    placeholder="Member Profiles"
                    class="w-48"
                    scroll-height="40rem"
                    dropdown-icon="pi pi-chart-line"
                    v-tooltip.bottom="'Profiles to mix.'"
                    :invalid="chosenMemberProfiles.length < 2"
                />
            </div>
            <div v-else-if="selectedType === ProfileType.Graph" class="flex flex-wrap justify-end">
                <div
                    class="p-2 flex leading-none items-center"
                    v-tooltip.bottom="
                        'Graph Profile Mouse actions:\n- Scroll to zoom.\n- Left-click on line to add a point.\n- Right-click on point to remove point.\n- Drag point to move.'
                    "
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiInformationSlabCircleOutline"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                </div>
                <div class="p-2 pr-1 flex flex-row">
                    <InputNumber
                        placeholder="Duty"
                        v-model="selectedDuty"
                        inputId="selected-duty"
                        mode="decimal"
                        class="duty-input w-full"
                        suffix="%"
                        showButtons
                        :min="dutyMin"
                        :max="dutyMax"
                        :disabled="selectedPointIndex == null && !showDutyKnob"
                        :use-grouping="false"
                        :step="1"
                        button-layout="horizontal"
                        :input-style="{ width: '5rem' }"
                        v-tooltip.bottom="'Selected Point Duty'"
                    >
                        <template #incrementicon>
                            <span class="pi pi-plus" />
                        </template>
                        <template #decrementicon>
                            <span class="pi pi-minus" />
                        </template>
                    </InputNumber>
                    <InputNumber
                        placeholder="Temp"
                        v-model="selectedTemp"
                        inputId="selected-temp"
                        mode="decimal"
                        class="temp-input w-full ml-3"
                        suffix="°"
                        showButtons
                        :min="inputNumberTempMin()"
                        :max="inputNumberTempMax()"
                        :disabled="selectedPointIndex == null && !showDutyKnob"
                        :use-grouping="false"
                        :step="0.1"
                        :min-fraction-digits="1"
                        :max-fraction-digits="1"
                        button-layout="horizontal"
                        :input-style="{ width: '5rem' }"
                        v-tooltip.bottom="'Selected Point Temp'"
                    >
                        <template #incrementicon>
                            <span class="pi pi-plus" />
                        </template>
                        <template #decrementicon>
                            <span class="pi pi-minus" />
                        </template>
                    </InputNumber>
                </div>
                <div class="p-2 pr-1">
                    <Select
                        v-model="chosenFunction"
                        :options="settingsStore.functions"
                        option-label="name"
                        placeholder="Function"
                        class="w-44 h-[2.375rem]"
                        checkmark
                        dropdown-icon="pi pi-directions"
                        scroll-height="40rem"
                        v-tooltip.bottom="'Function to apply'"
                    />
                </div>
                <div class="p-2 pr-0">
                    <Select
                        v-model="chosenTemp"
                        :options="tempSources"
                        class="w-44 h-[2.375rem]"
                        option-label="tempFrontendName"
                        option-group-label="deviceName"
                        option-group-children="temps"
                        placeholder="Temp Source"
                        filter-placeholder="Search"
                        filter
                        checkmark
                        scroll-height="40rem"
                        :invalid="chosenTemp == null || tempSourceInvalid"
                        dropdown-icon="pi pi-inbox"
                        v-tooltip.bottom="{ escape: false, value: 'Temperature Source' }"
                    >
                        <template #optiongroup="slotProps">
                            <div class="flex items-center">
                                <svg-icon
                                    type="mdi"
                                    :path="mdiMemory"
                                    :size="deviceStore.getREMSize(1.3)"
                                    class="mr-2"
                                />
                                <div>{{ slotProps.option.deviceName }}</div>
                            </div>
                        </template>
                        <template #option="slotProps">
                            <div class="flex w-full items-center justify-between">
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
                    </Select>
                </div>
            </div>
            <div v-else-if="selectedType === ProfileType.Fixed" class="p-2 pr-0">
                <InputNumber
                    placeholder="Duty"
                    v-model="selectedDuty"
                    inputId="selected-duty"
                    mode="decimal"
                    class="duty-input w-full h-[2.375rem]"
                    suffix="%"
                    showButtons
                    :min="dutyMin"
                    :max="dutyMax"
                    :disabled="selectedPointIndex == null && !showDutyKnob"
                    :use-grouping="false"
                    :step="1"
                    button-layout="horizontal"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.bottom="'Fixed Duty'"
                >
                    <template #incrementicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
            </div>
            <div class="p-2">
                <Select
                    v-model="selectedType"
                    :options="profileTypes"
                    placeholder="Profile Type"
                    class="w-24 h-[2.375rem] mr-3"
                    dropdown-icon="pi pi-chart-line"
                    scroll-height="400px"
                    checkmark
                    v-tooltip.bottom="{
                        escape: false,
                        value:
                            'Profile Type:<br/>' +
                            '- Default: Retains current device settings<br/>&nbsp;&nbsp;(BIOS/firmware)<br/>' +
                            '- Fixed: Sets a constant speed<br/>' +
                            '- Graph: Customizable fan curve<br/>' +
                            '- Mix: Combines multiple profiles',
                    }"
                />
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    label="Save"
                    v-tooltip.bottom="'Save Profile'"
                    @click="saveProfileState"
                >
                    <svg-icon
                        class="outline-0"
                        type="mdi"
                        :path="mdiContentSaveOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                </Button>
            </div>
        </div>
    </div>
    <!-- The UI Display: -->
    <div class="flex flex-col">
        <div v-if="selectedType === ProfileType.Default" class="text-center text-3xl m-8">
            Default
        </div>
        <Knob
            v-else-if="showDutyKnob"
            v-model="selectedDuty"
            class="duty-knob-input m-2 w-full h-full flex justify-center"
            :value-template="(value) => `${value}%`"
            :min="dutyMin"
            :max="dutyMax"
            :step="1"
            :stroke-width="deviceStore.getREMSize(0.75)"
            :size="deviceStore.getREMSize(27)"
        />
        <v-chart
            v-else-if="showGraph"
            id="control-graph"
            class="control-graph pt-6 pr-11 pl-4 pb-6"
            ref="controlGraph"
            :option="option"
            :autoresize="true"
            :manual-update="true"
            @contextmenu="deletePointFromLine"
            @zr:click="addPointToLine"
            @zr:contextmenu="deletePointFromLine"
        />
        <MixProfileEditorChart
            v-else-if="showMixChart"
            class="p-6"
            :profiles="chosenMemberProfiles"
            :mixFunctionType="chosenProfileMixFunction"
            :key="mixProfileKeys"
        />
    </div>
</template>

<style scoped lang="scss">
.control-graph {
    // todo: we might need a scrollable area:
    overflow: hidden;
    //height: max(95vh, 40rem);
    height: max(calc(100vh - 3.875rem), 40rem);
    //width: max(calc(90vw - 17rem), 30rem);
}

// This is needed particularly in Tauri, as it moves to multiline flex-wrap as soon as the scrollbar
//  appears. Other browsers don't do this, so we need to force it to nowrap.
//.grid-webkit-fix {
//    @media screen and (min-width: 38rem) {
//        -webkit-flex-wrap: nowrap;
//    }
//}
</style>
