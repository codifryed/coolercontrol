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
import VChart from 'vue-echarts'
import { mdiInformationSlabCircleOutline } from '@mdi/js'
import InputNumber from 'primevue/inputnumber'
import { computed, onMounted, onUnmounted, ref, Ref, watch, type WatchStopHandle } from 'vue'
import * as echarts from 'echarts/core'
import {
    DataZoomComponent,
    GraphicComponent,
    GridComponent,
    MarkAreaComponent,
    MarkPointComponent,
    TitleComponent,
    TooltipComponent,
} from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { CanvasRenderer } from 'echarts/renderers'
import { UniversalTransition } from 'echarts/features'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { useI18n } from 'vue-i18n'
import type { GraphicComponentLooseOption } from 'echarts/types/dist/shared'
import _ from 'lodash'
import { UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'

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

const props = defineProps<{
    profileUID: UID
}>()
const emit = defineEmits<{
    (e: 'changed', points: Array<[number, number]>): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const { t } = useI18n()

const currentProfile = computed(
    () => settingsStore.profiles.find((profile) => profile.uid === props.profileUID)!,
)

const selectedDuty: Ref<number | undefined> = ref()
const selectedOffset: Ref<number | undefined> = ref()
const selectedPointIndex: Ref<number | undefined> = ref()

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

const staticOffsetPrefix = computed(() =>
    selectedOffset.value != null && selectedOffset.value > 0 ? '+' : '',
)

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
                origin: 0,
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
    emit('changed', collectPoints())
}

const controlPointMotionForOffsetY = (posY: number, selectedPointIndex: number): void => {
    if (posY < offsetMin) {
        posY = offsetMin
    } else if (posY > offsetMax) {
        posY = offsetMax
    }
    data[selectedPointIndex].value[1] = posY
    emit('changed', collectPoints())
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
            { id: 'line-area', data: data },
        ],
        graphic: data.map((item, dataIndex) => ({
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
    option.series[1].areaStyle.color = new echarts.graphic.LinearGradient(
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
    emit('changed', collectPoints())
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
    option.series[1].areaStyle.color = new echarts.graphic.LinearGradient(
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
    emit('changed', collectPoints())
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
    const graphEl = document.getElementById('control-graph')
    const controlPanel = document.getElementById('control-panel')
    if (graphEl != null && controlPanel != null) {
        const panelHeight = controlPanel.getBoundingClientRect().height
        if (panelHeight > 56) {
            graphEl.style.height = `max(calc(100vh - (${panelHeight}px + 4.5rem)), 20rem)`
        } else {
            // 4rem panel height + 4rem for duty/temp bar
            graphEl.style.height = 'max(calc(100vh - 8rem), 20rem)'
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
</script>

<template>
    <div class="flex flex-row justify-center mt-4 mx-4 w-full">
        <div class="flex flex-row">
            <InputNumber
                :placeholder="t('common.duty')"
                v-model="selectedDuty"
                mode="decimal"
                class="duty-input h-11"
                :suffix="` ${t('common.percentUnit')}`"
                showButtons
                :min="dutyMin"
                :max="dutyMax"
                :disabled="
                    selectedPointIndex == null ||
                    selectedPointIndex >= data.length - 1 ||
                    selectedPointIndex === 0
                "
                :use-grouping="false"
                :step="1"
                button-layout="horizontal"
                :input-style="{ width: '5rem' }"
                v-tooltip.left="t('views.profiles.selectedPointOutputDuty')"
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
                :placeholder="t('common.offset')"
                v-model="selectedOffset"
                mode="decimal"
                class="offset-input h-11"
                :suffix="` ${t('common.percentUnit')}`"
                :prefix="staticOffsetPrefix"
                showButtons
                :min="offsetMin"
                :max="offsetMax"
                :disabled="selectedPointIndex == null"
                :use-grouping="false"
                :step="1"
                button-layout="horizontal"
                :input-style="{ width: '5rem' }"
                v-tooltip.right="t('views.profiles.selectedPointOffset')"
            >
                <template #incrementicon>
                    <span class="pi pi-plus" />
                </template>
                <template #decrementicon>
                    <span class="pi pi-minus" />
                </template>
            </InputNumber>
        </div>
    </div>
    <div id="profile-display" class="flex flex-col h-full">
        <v-chart
            id="control-graph"
            class="pt-6 pr-11 pl-4 pb-6"
            ref="controlGraph"
            :option="option"
            :autoresize="true"
            :manual-update="true"
            @contextmenu="deletePointFromLine"
            @zr:click="addPointToLine"
            @zr:contextmenu="deletePointFromLine"
        />
    </div>
</template>

<style scoped lang="scss">
#control-graph {
    overflow: hidden;
    // This is adjusted dynamically on resize with js above
    height: max(calc(100vh - 8rem), 20rem);
}
</style>
