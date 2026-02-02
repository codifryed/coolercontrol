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
import { Profile, ProfileType } from '@/models/Profile.ts'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
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
    offsetProfile: Array<[number, number]>
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'offsetProfile', offsetProfile: Array<[number, number]>): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const colors = useThemeColorsStore()

const currentProfile: Ref<Profile> = ref(new Profile(props.name, ProfileType.Overlay))
currentProfile.value.offset_profile = props.offsetProfile

const selectedDuty: Ref<number | undefined> = ref()
const selectedOffset: Ref<number | undefined> = ref()
const selectedPointIndex: Ref<number | undefined> = ref()
const tableDataKey: Ref<number> = ref(0)

// THIS IS ALMOST A STRAIGHT COPY from OverlayProfileEditorChart
//------------------------------------------------------------------------------------------------------------------------------------------
// User Control Graph
const defaultSymbolSize: number = deviceStore.getREMSize(1.0)
const defaultSymbolColor: string = colors.themeColors.bg_two
const selectedSymbolSize: number = deviceStore.getREMSize(1.25)
const selectedSymbolColor: string = colors.themeColors.accent
const dutyMin: number = 0
const dutyMax: number = 100
const offsetMin: number = -100
const offsetMax: number = 100
const profileMinLength: number = 2
const profileMaxLength: number = 12
const defaultProfileLength = 2

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
    const duties = lineSpace(dutyMin, dutyMax, defaultProfileLength, 0)
    const offsets = lineSpace(0, 0, defaultProfileLength, 0)
    for (const [index, duty] of duties.entries()) {
        result.push({
            value: [duty, offsets[index]],
            symbolSize: defaultSymbolSize,
            itemStyle: {
                color: defaultSymbolColor,
            },
        })
    }
    return result
}

const data: Array<PointData> = []
const initSeriesData = () => {
    data.length = 0
    if (
        currentProfile.value.offset_profile != null &&
        currentProfile.value.offset_profile.length > 1
    ) {
        for (const point of currentProfile.value.offset_profile) {
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

const collectPoints = (): Array<[number, number]> => {
    const points: Array<[number, number]> = []
    for (const pointData of data) {
        points.push(pointData.value)
    }
    return points
}

const graphicData: GraphicComponentLooseOption[] = []
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
            const duty = params.data.value[0].toFixed(0)
            const offsetValue = Math.round(params.data.value[1])
            const prefix = offsetValue >= 0 ? '+' : ''
            return (
                duty +
                t('common.percentUnit') +
                ' ' +
                prefix +
                offsetValue.toFixed(0) +
                t('common.percentUnit')
            )
        },
    },
    grid: {
        show: false,
        top: deviceStore.getREMSize(0.7),
        left: deviceStore.getREMSize(1.5),
        right: deviceStore.getREMSize(1.2),
        bottom: deviceStore.getREMSize(1.7),
        containLabel: true,
    },
    xAxis: {
        name: t('views.profiles.profileOutputDuty'),
        nameLocation: 'middle',
        nameGap: deviceStore.getREMSize(2.0),
        nameTextStyle: {
            color: colors.themeColors.text_color,
            fontSize: deviceStore.getREMSize(1.25),
        },
        min: dutyMin,
        max: dutyMax,
        type: 'value',
        splitNumber: 10,
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.95),
            formatter: (value: any): string => `${value}${t('common.percentUnit')}`,
        },
        axisLine: {
            onZero: false,
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
        name: t('views.profiles.offsetDuty'),
        nameLocation: 'middle',
        nameGap: deviceStore.getREMSize(3.25),
        nameTextStyle: {
            color: colors.themeColors.text_color,
            fontSize: deviceStore.getREMSize(1.25),
        },
        min: offsetMin,
        max: offsetMax,
        type: 'value',
        splitNumber: 10,
        cursor: 'no-drop',
        axisLabel: {
            fontSize: deviceStore.getREMSize(0.95),
            formatter: (value: any): string => {
                const prefix = value > 0 ? '+' : ''
                return `${prefix}${value}${t('common.percentUnit')}`
            },
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
            yAxisIndex: 0,
            filterMode: 'none',
            preventDefaultMouseMove: false,
            minValueSpan: 20,
            throttle: 25,
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
            // This is for the symbols that don't have draggable graphics on top, aka the last point
            cursor: 'no-drop',
            data: data,
        },
        {
            // Invisible wide line for easier click hit detection
            id: 'hit-area',
            type: 'line',
            smooth: 0.0,
            symbol: 'none',
            lineStyle: {
                color: 'transparent',
                width: 12,
            },
            emphasis: {
                disabled: true,
            },
            silent: false,
            z: 5,
            data: data,
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
                color: new echarts.graphic.LinearGradient(
                    0,
                    1,
                    0,
                    0, // this point is set once graph is rendered
                    [
                        {
                            offset: 0,
                            color: colors.convertColorToRGBA(colors.themeColors.accent, 0.8),
                        },
                        {
                            offset: 0.5,
                            color: colors.convertColorToRGBA(colors.themeColors.accent, 0.2),
                        },
                        {
                            offset: 1,
                            color: colors.convertColorToRGBA(colors.themeColors.accent, 0.8),
                        },
                    ],
                    true,
                ),
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

const controlPointMotionForDutyX = (posX: number, selectedPointIndex: number): void => {
    // We use 1 whole degree of separation between points so point index works perfect:
    const minActivePosition = dutyMin + selectedPointIndex
    const maxActivePosition = dutyMax - (data.length - (selectedPointIndex + 1))
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
        const indexDiff = i - selectedPointIndex // index difference = 1% duty difference
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
    emit('offsetProfile', collectPoints())
}

const controlPointMotionForOffsetY = (posY: number, selectedPointIndex: number): void => {
    if (posY < offsetMin) {
        posY = offsetMin
    } else if (posY > offsetMax) {
        posY = offsetMax
    }
    data[selectedPointIndex].value[1] = posY
    emit('offsetProfile', collectPoints())
}

//----------------------------------------------------------------------------------------------------------------------
const controlGraph = ref<InstanceType<typeof VChart> | null>(null)

const setDutyAndOffsetValues = (dataIndex: number): void => {
    selectedDuty.value = deviceStore.round(data[dataIndex].value[0])
    selectedOffset.value = deviceStore.round(data[dataIndex].value[1])
}

const onPointDragging = (dataIndex: number, posXY: [number, number]): void => {
    // Point dragging needs to be very fast and efficient. We'll set the points to their allowed positions on drag end
    data[dataIndex].value = posXY
    controlGraph.value?.setOption({
        series: [
            { id: 'a', data: data },
            { id: 'hit-area', data: data },
            { id: 'line-area', data: data },
        ],
    })
}

const afterPointDragging = (dataIndex: number, posXY: [number, number]): void => {
    // what needs to happen AFTER the dragging is done:
    controlPointMotionForDutyX(posXY[0], dataIndex)
    controlPointMotionForOffsetY(posXY[1], dataIndex)
    controlGraph.value?.setOption({
        series: [
            { id: 'a', data: data },
            { id: 'hit-area', data: data },
            { id: 'line-area', data: data },
        ],
        graphic: data.map((item, dataIndex) => ({
            id: dataIndex,
            type: 'circle',
            position: controlGraph.value?.convertToPixel('grid', item.value),
        })),
    })
    tableDataKey.value++
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

const createWatcherOfDutyOffsetText = (): WatchStopHandle =>
    watch(
        [selectedDuty, selectedOffset],
        (newDutyAndOffset) => {
            if (selectedPointIndex.value == null) {
                return
            }
            controlPointMotionForDutyX(newDutyAndOffset[0]!, selectedPointIndex.value)
            controlPointMotionForOffsetY(newDutyAndOffset[1]!, selectedPointIndex.value)
            data.forEach(
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
let dutyOffsetTextWatchStopper = createWatcherOfDutyOffsetText()

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
                setDutyAndOffsetValues(dataIndex)
                this.cursor = 'grab'
            },
            ondragend: function () {
                // the only real benefit of ondragend, is that it works even when the point has moved out of scope of the graph
                const [posX, posY] = (controlGraph.value?.convertFromPixel('grid', [
                    (this as any).x,
                    (this as any).y,
                ]) as [number, number]) ?? [0, 0]
                if (posX < dutyMin || posX > dutyMax || posY < offsetMin || posY > offsetMax) {
                    afterPointDragging(dataIndex, [posX, posY])
                    setDutyAndOffsetValues(dataIndex)
                    this.cursor = 'grab'
                }
            },
            onmouseover: function (eChartEvent: any) {
                if (eChartEvent?.event?.buttons !== 0) {
                    // EChart button numbers are different. 0=None, 1=Left, 2=Right
                    return // only react when no buttons are pressed (better drag UX)
                }
                dutyOffsetTextWatchStopper()
                setDutyAndOffsetValues(dataIndex)
                selectedPointIndex.value = dataIndex // sets the selected point on move over
                showTooltip(dataIndex)
                this.cursor = 'grab'
            },
            onmouseout: function (eChartEvent: any) {
                if (eChartEvent?.event?.buttons !== 0) {
                    return // only react when no buttons are pressed (better drag UX)
                }
                dutyOffsetTextWatchStopper() // make sure we stop and runny watchers before changing the reference
                dutyOffsetTextWatchStopper = createWatcherOfDutyOffsetText()
                hideTooltip()
            },
            z: 100,
        }
    }
    // clear and push
    graphicData.length = 0
    graphicData.push(
        ...data.map((item, dataIndex) => createGraphicDataForPoint(dataIndex, item.value)),
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
    if (data.length >= profileMaxLength) {
        return
    }
    selectedPointIndex.value = undefined
    const posXY = (controlGraph.value?.convertFromPixel('grid', [
        params.offsetX,
        params.offsetY,
    ]) as [number, number]) ?? [0, 0]
    // Clamp offset to min/max (with hi-res graphs, sometimes it went out of max bounds)
    posXY[1] = Math.min(Math.max(posXY[1], offsetMin), offsetMax)
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
    // @ts-ignore
    option.series[2].areaStyle.color = new echarts.graphic.LinearGradient(
        0,
        0,
        0,
        controlGraph.value?.convertToPixel('grid', [0, -100])[1] ?? 100,
        [
            {
                offset: 0,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.8),
            },
            {
                offset: 0.5,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.3),
            },
            {
                offset: 1,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.8),
            },
        ],
        true,
    )
    controlGraph.value?.setOption(option)
    // select the new point under the cursor:
    dutyOffsetTextWatchStopper()
    setDutyAndOffsetValues(indexToInsertAt)
    // this needs a bit of time for the graph to refresh before being set correctly:
    setTimeout(() => (selectedPointIndex.value = indexToInsertAt), 50)
    setTimeout(() => showTooltip(indexToInsertAt), 350) // wait until point animation is complete before showing tooltip
    tableDataKey.value++
    emit('offsetProfile', collectPoints())
}

const deletePointFromLine = (params: any) => {
    if (params.componentType !== 'graphic' || params.event?.target?.id == null) {
        if (params.stop) {
            params.stop() // this stops any context menu from appearing in the graph
        }
        return
    }
    params.event.stop()
    if (data.length <= profileMinLength) {
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
    // @ts-ignore
    option.series[2].areaStyle.color = new echarts.graphic.LinearGradient(
        0,
        0,
        0,
        controlGraph.value?.convertToPixel('grid', [0, -100])[1] ?? 100,
        [
            {
                offset: 0,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.8),
            },
            {
                offset: 0.5,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.3),
            },
            {
                offset: 1,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.8),
            },
        ],
        true,
    )
    controlGraph.value?.setOption(option, { replaceMerge: ['series', 'graphic'], silent: true })
    tableDataKey.value++
    emit('offsetProfile', collectPoints())
}

//--------------------------------------------------------------------------------------------------
// Points Table Overlay

// Minimum duty separation between points (1%)
const MIN_DUTY_SEPARATION = 1

// Points table position (local state, not persisted)
type TablePosition = 'top-left' | 'bottom-right'
const tablePosition: Ref<TablePosition> = ref('top-left')

const tablePositionClasses = computed(() => ({
    'top-14 left-[7.5rem]': tablePosition.value === 'top-left',
    'bottom-20 right-[4.5rem]': tablePosition.value === 'bottom-right',
}))

const cycleTablePosition = () => {
    tablePosition.value = tablePosition.value === 'top-left' ? 'bottom-right' : 'top-left'
}

// Select point from table
const selectPointFromTable = (idx: number): void => {
    dutyOffsetTextWatchStopper()
    selectedPointIndex.value = idx
    setDutyAndOffsetValues(idx)
}

// Calculate min/max duty for a specific point index (for table editing)
const getPointDutyMin = (idx: number): number => {
    return dutyMin + idx
}

const getPointDutyMax = (idx: number): number => {
    if (idx === 0) return dutyMin
    if (idx === data.length - 1) return dutyMax
    return dutyMax - (data.length - 1 - idx)
}

// Update point value from table (reuses existing constraint functions)
const updatePointFromTable = (idx: number, newDuty: number, newOffset: number): void => {
    selectPointFromTable(idx)
    controlPointMotionForDutyX(newDuty, idx)
    controlPointMotionForOffsetY(newOffset, idx)
    refreshGraphAfterTableEdit()
}

const refreshGraphAfterTableEdit = (): void => {
    createGraphicDataFromPointData()
    controlGraph.value?.setOption({
        series: [
            { id: 'a', data: data },
            { id: 'hit-area', data: data },
            { id: 'line-area', data: data },
        ],
        graphic: graphicData,
    })
    tableDataKey.value++
}

// Increment/decrement handlers for table cells
const incrementPointDuty = (idx: number): void => {
    const newDuty = Math.min(data[idx].value[0] + 1, getPointDutyMax(idx))
    updatePointFromTable(idx, newDuty, data[idx].value[1])
}

const decrementPointDuty = (idx: number): void => {
    const newDuty = Math.max(data[idx].value[0] - 1, getPointDutyMin(idx))
    updatePointFromTable(idx, newDuty, data[idx].value[1])
}

const incrementPointOffset = (idx: number): void => {
    const newOffset = Math.min(data[idx].value[1] + 1, offsetMax)
    updatePointFromTable(idx, data[idx].value[0], newOffset)
}

const decrementPointOffset = (idx: number): void => {
    const newOffset = Math.max(data[idx].value[1] - 1, offsetMin)
    updatePointFromTable(idx, data[idx].value[0], newOffset)
}

// Scroll wheel handlers for table cells
const handleDutyTableScroll = (event: WheelEvent, idx: number): void => {
    event.preventDefault()
    if (event.deltaY < 0) incrementPointDuty(idx)
    else decrementPointDuty(idx)
}

const handleOffsetTableScroll = (event: WheelEvent, idx: number): void => {
    event.preventDefault()
    if (event.deltaY < 0) incrementPointOffset(idx)
    else decrementPointOffset(idx)
}

// Direct input handlers for table cells
const handleDutyTableInput = (idx: number, value: number | null): void => {
    if (value == null || idx === 0 || idx === data.length - 1) return
    const clampedDuty = Math.max(getPointDutyMin(idx), Math.min(value, getPointDutyMax(idx)))
    updatePointFromTable(idx, clampedDuty, data[idx].value[1])
}

const handleOffsetTableInput = (idx: number, value: number | null): void => {
    if (value == null) return
    const clampedOffset = Math.max(offsetMin, Math.min(value, offsetMax))
    updatePointFromTable(idx, data[idx].value[0], clampedOffset)
}

// Press-and-hold repeat functionality for increment/decrement buttons
let repeatTimeout: ReturnType<typeof setTimeout> | null = null
let repeatInterval: ReturnType<typeof setInterval> | null = null
const REPEAT_DELAY = 400 // Initial delay before repeat starts (ms)
const REPEAT_RATE = 75 // Interval between repeats (ms)

const startRepeat = (action: () => void): void => {
    stopRepeat()
    action() // Execute immediately on press
    repeatTimeout = setTimeout(() => {
        repeatInterval = setInterval(action, REPEAT_RATE)
    }, REPEAT_DELAY)
}

const stopRepeat = (): void => {
    if (repeatTimeout) {
        clearTimeout(repeatTimeout)
        repeatTimeout = null
    }
    if (repeatInterval) {
        clearInterval(repeatInterval)
        repeatInterval = null
    }
}

// Add point after the selected index
const addPointFromTable = (afterIdx: number): void => {
    if (data.length >= profileMaxLength) return
    if (afterIdx >= data.length - 1) return // Can't add after last point

    // Calculate midpoint between current and next point
    const currentPoint = data[afterIdx].value
    const nextPoint = data[afterIdx + 1].value

    // Ensure minimum duty separation
    const dutyGap = nextPoint[0] - currentPoint[0]
    const requiredGap = MIN_DUTY_SEPARATION * 2

    // If gap is too small, try to make room by moving adjacent points
    if (dutyGap < requiredGap) {
        const deficit = requiredGap - dutyGap

        // Calculate how much room we have to move each point
        let lowerMinDuty: number
        if (afterIdx === 0) {
            lowerMinDuty = dutyMin
        } else {
            lowerMinDuty = data[afterIdx - 1].value[0] + MIN_DUTY_SEPARATION
        }
        const lowerRoom = Math.max(0, currentPoint[0] - lowerMinDuty)

        let upperMaxDuty: number
        if (afterIdx + 1 === data.length - 1) {
            upperMaxDuty = dutyMax
        } else {
            upperMaxDuty = data[afterIdx + 2].value[0] - MIN_DUTY_SEPARATION
        }
        const upperRoom = Math.max(0, upperMaxDuty - nextPoint[0])

        // Check if we have enough total room
        if (lowerRoom + upperRoom < deficit) return // Can't make enough room

        // Move points to make room
        const lowerMove = Math.min(lowerRoom, deficit)
        const upperMove = deficit - lowerMove
        if (lowerMove > 0) currentPoint[0] -= lowerMove
        if (upperMove > 0) nextPoint[0] += upperMove
    }

    const newDuty = (currentPoint[0] + nextPoint[0]) / 2
    const newOffset = (currentPoint[1] + nextPoint[1]) / 2

    data.splice(afterIdx + 1, 0, {
        value: [newDuty, newOffset],
        symbolSize: selectedSymbolSize,
        itemStyle: { color: selectedSymbolColor },
    })

    createGraphicDataFromPointData()
    // @ts-ignore
    option.series[0].data = data
    // @ts-ignore
    option.graphic = graphicData
    // @ts-ignore
    option.series[2].areaStyle.color = new echarts.graphic.LinearGradient(
        0,
        0,
        0,
        controlGraph.value?.convertToPixel('grid', [0, -100])[1] ?? 100,
        [
            {
                offset: 0,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.8),
            },
            {
                offset: 0.5,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.3),
            },
            {
                offset: 1,
                color: colors.convertColorToRGBA(colors.themeColors.accent, 0.8),
            },
        ],
        true,
    )
    controlGraph.value?.setOption(option)

    selectedPointIndex.value = afterIdx + 1
    setDutyAndOffsetValues(afterIdx + 1)
    tableDataKey.value++
    emit('offsetProfile', collectPoints())
}

// Remove point at index
const removePointFromTable = (idx: number): void => {
    if (data.length <= profileMinLength) return
    if (idx === 0 || idx === data.length - 1) return // Can't remove first/last

    data.splice(idx, 1)
    selectedPointIndex.value = undefined
    hideTooltip()
    createGraphicDataFromPointData()
    // @ts-ignore
    option.series[0].data = data
    // @ts-ignore
    option.graphic = graphicData
    controlGraph.value?.setOption(option, { replaceMerge: ['series', 'graphic'], silent: true })
    tableDataKey.value++
    emit('offsetProfile', collectPoints())
}

// Check if point can be removed
const canRemovePoint = (idx: number): boolean => {
    return data.length > profileMinLength && idx !== 0 && idx !== data.length - 1
}

// Check if point can be added after this index
const canAddPointAfter = (idx: number): boolean => {
    if (data.length >= profileMaxLength) return false
    if (idx >= data.length - 1) return false

    const currentPoint = data[idx].value
    const nextPoint = data[idx + 1].value
    const dutyGap = nextPoint[0] - currentPoint[0]
    const requiredGap = MIN_DUTY_SEPARATION * 2

    if (dutyGap >= requiredGap) return true

    // Check if we can make room by moving adjacent points
    const deficit = requiredGap - dutyGap
    let lowerMinDuty: number
    if (idx === 0) {
        lowerMinDuty = dutyMin
    } else {
        lowerMinDuty = data[idx - 1].value[0] + MIN_DUTY_SEPARATION
    }
    const lowerRoom = Math.max(0, currentPoint[0] - lowerMinDuty)

    let upperMaxDuty: number
    if (idx + 1 === data.length - 1) {
        upperMaxDuty = dutyMax
    } else {
        upperMaxDuty = data[idx + 2].value[0] - MIN_DUTY_SEPARATION
    }
    const upperRoom = Math.max(0, upperMaxDuty - nextPoint[0])

    return lowerRoom + upperRoom >= deficit
}

//--------------------------------------------------------------------------------------------------

const dutyScrolled = (event: WheelEvent): void => {
    if (selectedDuty.value == null) return
    if (event.deltaY < 0) {
        if (selectedDuty.value < dutyMax) selectedDuty.value += 1
    } else {
        if (selectedDuty.value > dutyMin) selectedDuty.value -= 1
    }
}
const offsetScrolled = (event: WheelEvent): void => {
    if (selectedOffset.value == null) return
    if (event.deltaY < 0) {
        if (selectedOffset.value < offsetMax) selectedOffset.value += 1
    } else {
        if (selectedOffset.value > offsetMin) selectedOffset.value -= 1
    }
}
const updateResponsiveGraphHeight = (): void => {
    const graphEl = document.getElementById('control-graph-wiz')
    const controlPanel = document.getElementById('control-panel')
    if (graphEl != null && controlPanel != null) {
        const panelHeight = controlPanel.getBoundingClientRect().height
        if (panelHeight > 56) {
            graphEl.style.height = `max(calc(80vh - (${panelHeight}px + 4.5rem)), 20rem)`
        } else {
            // 4rem panel height + 4rem for duty/temp bar
            graphEl.style.height = 'max(calc(80vh - 10rem), 20rem)'
        }
        controlGraph.value?.setOption({
            series: [
                {
                    id: 'line-area',
                    areaStyle: {
                        color: new echarts.graphic.LinearGradient(
                            0,
                            0,
                            0,
                            controlGraph.value?.convertToPixel('grid', [0, -100])[1],
                            [
                                {
                                    offset: 0,
                                    color: colors.convertColorToRGBA(
                                        colors.themeColors.accent,
                                        0.8,
                                    ),
                                },
                                {
                                    offset: 0.5,
                                    color: colors.convertColorToRGBA(
                                        colors.themeColors.accent,
                                        0.3,
                                    ),
                                },
                                {
                                    offset: 1,
                                    color: colors.convertColorToRGBA(
                                        colors.themeColors.accent,
                                        0.8,
                                    ),
                                },
                            ],
                            true,
                        ),
                    },
                },
            ],
        })
    }
}
const updatePosition = (): void => {
    controlGraph.value?.setOption({
        graphic: data.map((item, dataIndex) => ({
            id: dataIndex,
            position: controlGraph.value?.convertToPixel('grid', item.value),
        })),
    })
}
//----------------------------------------------------------------------------------------------------------------------

const addScrollEventListeners = (): void => {
    // @ts-ignore
    document?.querySelector('.duty-input')?.addEventListener('wheel', dutyScrolled)
    // @ts-ignore
    document?.querySelector('.offset-input')?.addEventListener('wheel', offsetScrolled)
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
    addScrollEventListeners()

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
})
onUnmounted(() => {
    window.removeEventListener('resize', updateResponsiveGraphHeight)
    window.removeEventListener('resize', updatePosition)
})

const nextStep = () => {
    emit('offsetProfile', collectPoints())
    emit('nextStep', 13)
}
</script>

<template>
    <div id="control-panel" class="flex flex-col w-[87vw]">
        <div id="profile-display" class="bg-bg-one rounded-lg relative">
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
            <!-- Points Table Overlay -->
            <div
                class="absolute z-10 bg-bg-two/90 border border-border-one rounded-lg shadow-lg max-h-[calc(100vh-6rem)] overflow-y-auto"
                :class="tablePositionClasses"
            >
                <div
                    class="flex justify-between items-center px-2 py-1 border-b border-border-one sticky top-0 bg-bg-two/95"
                >
                    <span class="font-semibold text-text-color cursor-default">{{
                        t('views.profiles.points')
                    }}</span>
                    <Button
                        @click="cycleTablePosition"
                        icon="pi pi-arrow-up-right-and-arrow-down-left-from-center rotate-90"
                        text
                        rounded
                        class="!w-7 !h-7 !p-0"
                        v-tooltip.top="t('views.profiles.moveTable')"
                    />
                </div>
                <table class="w-full">
                    <thead class="sticky top-7 bg-bg-two/95 cursor-default">
                        <tr class="text-text-color-secondary">
                            <th class="px-2 py-1 text-left">#</th>
                            <th class="px-1 py-1 text-center">{{ t('common.duty') }}</th>
                            <th class="px-1 py-1 text-center">{{ t('common.offset') }}</th>
                            <th class="px-1 py-1 w-6"></th>
                        </tr>
                    </thead>
                    <tbody :key="tableDataKey">
                        <tr
                            v-for="(point, idx) in data"
                            :key="`${tableDataKey}-${idx}`"
                            class="group"
                            :class="{
                                'bg-accent/30': idx === selectedPointIndex,
                                'hover:bg-bg-one/20': idx !== selectedPointIndex,
                            }"
                        >
                            <!-- Point Index -->
                            <td
                                class="px-2 py-0.5 text-text-color-secondary cursor-pointer"
                                @click="selectPointFromTable(idx)"
                            >
                                {{ idx + 1 }}
                            </td>

                            <!-- Duty Cell with +/- buttons -->
                            <td class="pr-2 py-1">
                                <div
                                    class="flex items-center justify-center gap-0.5"
                                    @wheel.prevent="
                                        idx !== 0 &&
                                        idx !== data.length - 1 &&
                                        handleDutyTableScroll($event, idx)
                                    "
                                >
                                    <Button
                                        icon="pi pi-minus"
                                        text
                                        size="small"
                                        class="!w-5 !h-5 !p-0 [&>span]:text-[0.6rem]"
                                        :disabled="
                                            idx === 0 ||
                                            idx === data.length - 1 ||
                                            data[idx].value[0] <= getPointDutyMin(idx)
                                        "
                                        @pointerdown.stop="
                                            startRepeat(() => decrementPointDuty(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    />
                                    <InputNumber
                                        :modelValue="point.value[0]"
                                        @update:modelValue="handleDutyTableInput(idx, $event)"
                                        @focus="selectPointFromTable(idx)"
                                        mode="decimal"
                                        :minFractionDigits="0"
                                        :maxFractionDigits="0"
                                        :min="getPointDutyMin(idx)"
                                        :max="getPointDutyMax(idx)"
                                        :suffix="t('common.percentUnit')"
                                        :disabled="idx === 0 || idx === data.length - 1"
                                        :inputStyle="{
                                            width: '3rem',
                                            textAlign: 'center',
                                            padding: '0.125rem',
                                        }"
                                        class="table-input"
                                    />
                                    <Button
                                        icon="pi pi-plus"
                                        text
                                        size="small"
                                        class="!w-5 !h-5 !p-0 [&>span]:text-[0.6rem]"
                                        :disabled="
                                            idx === 0 ||
                                            idx === data.length - 1 ||
                                            data[idx].value[0] >= getPointDutyMax(idx)
                                        "
                                        @pointerdown.stop="
                                            startRepeat(() => incrementPointDuty(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    />
                                </div>
                            </td>

                            <!-- Offset Cell with +/- buttons -->
                            <td class="pr-2 py-1">
                                <div
                                    class="flex items-center justify-center gap-0.5"
                                    @wheel.prevent="handleOffsetTableScroll($event, idx)"
                                >
                                    <Button
                                        icon="pi pi-minus"
                                        text
                                        size="small"
                                        class="!w-5 !h-5 !p-0 [&>span]:text-[0.6rem]"
                                        :disabled="data[idx].value[1] <= offsetMin"
                                        @pointerdown.stop="
                                            startRepeat(() => decrementPointOffset(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    />
                                    <InputNumber
                                        :modelValue="point.value[1]"
                                        @update:modelValue="handleOffsetTableInput(idx, $event)"
                                        @focus="selectPointFromTable(idx)"
                                        mode="decimal"
                                        :minFractionDigits="0"
                                        :maxFractionDigits="0"
                                        :min="offsetMin"
                                        :max="offsetMax"
                                        :suffix="t('common.percentUnit')"
                                        :inputStyle="{
                                            width: '3.5rem',
                                            textAlign: 'center',
                                            padding: '0.125rem',
                                        }"
                                        class="table-input"
                                    />
                                    <Button
                                        icon="pi pi-plus"
                                        text
                                        size="small"
                                        class="!w-5 !h-5 !p-0 [&>span]:text-[0.6rem]"
                                        :disabled="data[idx].value[1] >= offsetMax"
                                        @pointerdown.stop="
                                            startRepeat(() => incrementPointOffset(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    />
                                </div>
                            </td>

                            <!-- Action buttons (add/remove) -->
                            <td class="px-1 py-0.5">
                                <div
                                    class="flex gap-0.5 opacity-0 group-hover:opacity-100"
                                >
                                    <Button
                                        v-if="canAddPointAfter(idx)"
                                        icon="pi pi-plus-circle"
                                        text
                                        severity="success"
                                        size="small"
                                        class="!w-6 !h-6 !p-0"
                                        v-tooltip.top="t('views.profiles.addPointAfter')"
                                        @click.stop="addPointFromTable(idx)"
                                    />
                                    <Button
                                        v-if="canRemovePoint(idx)"
                                        icon="pi pi-trash"
                                        text
                                        severity="danger"
                                        size="small"
                                        class="!w-6 !h-6 !p-0"
                                        v-tooltip.top="t('views.profiles.removePoint')"
                                        @click.stop="removePointFromTable(idx)"
                                    />
                                </div>
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
        <div class="flex flex-row justify-center mt-4">
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
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 3)">
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

// Compact styling for points table InputNumber components
:deep(.table-input) {
    input {
        background: transparent;
        border: none;
        height: 1.5rem;

        &:focus {
            box-shadow: none;
            background: var(--cc-bg-one);
        }

        &:disabled {
            opacity: 0.6;
        }
    }
}
</style>
