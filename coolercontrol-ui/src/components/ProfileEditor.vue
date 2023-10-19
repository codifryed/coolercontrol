<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">

import {useSettingsStore} from "@/stores/SettingsStore"
import {Function, ProfileType, ProfileTempSource} from "@/models/Profile"
import Button from 'primevue/button'
import Dropdown from 'primevue/dropdown'
import {computed, onMounted, type Ref, ref, watch, type WatchStopHandle} from "vue";
import InputText from 'primevue/inputtext'
import InputNumber from 'primevue/inputnumber'
import Knob from 'primevue/knob'
import {useConfirm} from "primevue/useconfirm"
import {useDeviceStore} from "@/stores/DeviceStore"
import * as echarts from 'echarts/core'
import {GraphicComponent, GridComponent, MarkAreaComponent, TooltipComponent,} from 'echarts/components'
import {LineChart} from 'echarts/charts'
import {UniversalTransition} from 'echarts/features'
import {CanvasRenderer} from 'echarts/renderers'
import VChart from 'vue-echarts'
import {type EChartsOption} from "echarts"
import {type GraphicComponentLooseOption} from "echarts/types/dist/shared"
import {useThemeColorsStore} from "@/stores/ThemeColorsStore"
import {storeToRefs} from "pinia"
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {mdiContentSaveMoveOutline} from "@mdi/js"
import {$enum} from "ts-enum-util";

echarts.use([
  GridComponent, LineChart, CanvasRenderer, UniversalTransition, TooltipComponent, GraphicComponent, MarkAreaComponent
])


interface Props {
  profileUID: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
  profileChange: [value: boolean]
}>()

const deviceStore = useDeviceStore()
const {currentDeviceStatus} = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const confirm = useConfirm()

const currentProfile = computed(() => settingsStore.profiles.find((profile) => profile.uid === props.profileUID)!)
const givenName: Ref<string> = ref(currentProfile.value.name)
const selectedType: Ref<ProfileType> = ref(currentProfile.value.p_type)
const profileTypes = [...$enum(ProfileType).keys()]
const speedProfile: Ref<Array<[number, number]>> = ref(currentProfile.value.speed_profile)
const speedDuty: Ref<number | undefined> = ref(currentProfile.value.speed_fixed)

interface AvailableTemp {
  deviceUID: string // needed here as well for the dropdown selector
  tempName: string
  tempFrontendName: string
  tempExternalName: string
  lineColor: string
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
  tempExternalName: string
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
      deviceName: device.nameShort,
      profileMinLength: device.info.profile_min_length,
      profileMaxLength: device.info.profile_max_length,
      tempMin: device.info.temp_min,
      tempMax: device.info.temp_max,
      temps: [],
    }
    for (const temp of device.status.temps) {
      if (deviceSettings.sensorsAndChannels.getValue(temp.name).hide) {
        continue
      }
      deviceSource.temps.push({
        deviceUID: device.uid,
        tempName: temp.name,
        tempFrontendName: temp.frontend_name,
        tempExternalName: temp.external_name,
        lineColor: deviceSettings.sensorsAndChannels.getValue(temp.name).color
      });
    }
    if (deviceSource.temps.length === 0) {
      continue // when all of a devices temps are hidden
    }
    tempSources.value.push(deviceSource)
  }
}
fillTempSources()

const getCurrentTempSource = (deviceUID: string | undefined, tempName: string | undefined): CurrentTempSource | undefined => {
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
      tempExternalName: tmpTemp.tempExternalName,
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
    settingsStore.functions.find(f => f.uid === currentProfile.value.function_uid)!
)
const selectedTemp: Ref<number | undefined> = ref()
const selectedDuty: Ref<number | undefined> = ref()
const selectedPointIndex: Ref<number | undefined> = ref()
const settingsChanged: Ref<boolean> = ref(false)
const selectedTempSourceTemp: Ref<number | undefined> = ref()

//------------------------------------------------------------------------------------------------------------------------------------------
// User Control Graph

const defaultSymbolSize: number = 15
const defaultSymbolColor: string = colors.themeColors().bg_three
const selectedSymbolSize: number = 20
const selectedSymbolColor: string = colors.themeColors().green
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

const lineSpace = (startValue: number, stopValue: number, cardinality: number, precision: number): Array<number> => {
  const arr = []
  const step = (stopValue - startValue) / (cardinality - 1)
  for (let i = 0; i < cardinality; i++) {
    const value = startValue + (step * i)
    arr.push(deviceStore.round(value, precision))
  }
  return arr
}

const defaultDataValues = (): Array<PointData> => {
  const result: Array<PointData> = []
  if (selectedTempSource != null) {
    const profileLength = selectedTempSource.profileMinLength <= 5 && selectedTempSource.profileMaxLength >= 5
        ? 5 : selectedTempSource.profileMaxLength;
    const temps = lineSpace(selectedTempSource.tempMin, selectedTempSource.tempMax, profileLength, 1)
    const duties = lineSpace(dutyMin, dutyMax, profileLength, 0)
    for (const [index, temp] of temps.entries()) {
      result.push({
        value: [temp, duties[index]],
        symbolSize: defaultSymbolSize,
        itemStyle: {
          color: defaultSymbolColor,
        }
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
        }
      })
    }
  }
  return result
}

const data: Array<PointData> = []
const initSeriesData = () => {
  data.length = 0
  if (speedProfile.value.length > 1 && selectedTempSource != null) {
    for (const point of speedProfile.value) {
      data.push({
        value: point,
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

const markAreaData: [({
  xAxis: number
})[], ({
  xAxis: number
})[]] = [
  [{xAxis: axisXTempMin}, {xAxis: axisXTempMin}],
  [{xAxis: axisXTempMax}, {xAxis: axisXTempMax}]
]

const graphicData: GraphicComponentLooseOption[] = []

const tempLineData: [{
  value: number[]
}, {
  value: number[]
}] = [{value: []}, {value: []}]

const initOptions = {
  useDirtyRect: false, // true unfortunately causes artifacts and doesn't speed this use case up at all
  renderer: 'canvas',
}

const option: EChartsOption = {
  tooltip: {
    triggerOn: 'none',
    borderWidth: 1,
    borderColor: colors.themeColors().text_foreground + 'FF',
    backgroundColor: colors.themeColors().bg_two + 'F0',
    textStyle: {
      color: colors.themeColors().green,
      fontSize: 14,
    },
    padding: 3,
    transitionDuration: 0.3,
    formatter: function (params: any) {
      return (
          params.data.value[1].toFixed(0) + '% ' +
          params.data.value[0].toFixed(1) + '°'
      )
    }
  },
  grid: {
    show: false,
    top: 10,
    left: 10,
    right: 15,
    bottom: 0,
    containLabel: true,
  },
  xAxis: {
    min: axisXTempMin,
    max: axisXTempMax,
    type: 'value',
    splitNumber: 10,
    axisLabel: {
      formatter: '{value}°'
    },
    axisLine: {
      lineStyle: {
        color: colors.themeColors().text_active,
        width: 1,
      }
    },
    splitLine: {
      lineStyle: {
        color: colors.themeColors().text_description,
        type: 'dotted'
      }
    },
  },
  yAxis: {
    min: dutyMin,
    max: dutyMax,
    type: 'value',
    axisLabel: {
      formatter: '{value}%'
    },
    axisLine: {
      lineStyle: {
        color: colors.themeColors().text_active,
        width: 1,
      }
    },
    splitLine: {
      lineStyle: {
        color: colors.themeColors().text_description,
        type: 'dotted'
      }
    },
  },
  // @ts-ignore
  series: [
    {
      id: 'a',
      type: 'line',
      smooth: false,
      symbol: 'circle',
      symbolSize: 15,
      itemStyle: {
        color: colors.themeColors().bg_three,
        borderColor: colors.themeColors().green,
        borderWidth: 2,
      },
      lineStyle: {
        color: colors.themeColors().green,
        width: 2,
        type: 'solid',
      },
      emphasis: {
        disabled: true, // won't work anyway with our draggable graphics that lay on top
      },
      markArea: {
        silent: true,
        itemStyle: {
          color: colors.themeColors().red,
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
      data: data
    },
    {
      id: 'tempLine',
      type: 'line',
      smooth: false,
      symbol: 'none',
      // No label for now
      // endLabel: {
      //   show: true,
      //   fontSize: 12,
      //   color: colors.themeColors().yellow,
      //   // rotate: 90,
      //   // offset: [-35, -15],
      //   offset: [-23, -5],
      //   valueAnimation: false,
      //   // @ts-ignore
      //   formatter: (params) => params.value[0].toFixed(1) + '°',
      // },
      lineStyle: {
        color: colors.themeColors().yellow,
        width: 1,
        type: 'dashed',
      },
      emphasis: {
        disabled: true,
      },
      data: tempLineData,
      z: 1,
      silent: true,
    }
  ],
  animation: true,
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
  markAreaData[0] = [{xAxis: axisXTempMin}, {xAxis: selectedTempSource!.tempMin}]
  markAreaData[1] = [{xAxis: selectedTempSource!.tempMax}, {xAxis: axisXTempMax}]
  selectedTempSourceTemp.value = Number(deviceStore
      .currentDeviceStatus
      .get(selectedTempSource.deviceUID)
      ?.get(selectedTempSource.tempFrontendName)
      ?.temp)
  // @ts-ignore
  option.series[1].lineStyle.color = selectedTempSource.color
  // @ts-ignore
  // option.series[1].endLabel.color = selectedTempSource.color
  tempLineData[0].value = [selectedTempSourceTemp.value, dutyMin]
  tempLineData[1].value = [selectedTempSourceTemp.value, dutyMax]
}
if (selectedTempSource != null) { // set chosenTemp on startup if set in profile
  chosenTemp.value = {
    deviceUID: selectedTempSource.deviceUID,
    lineColor: selectedTempSource.color,
    tempExternalName: selectedTempSource.tempExternalName,
    tempFrontendName: selectedTempSource.tempFrontendName,
    tempName: selectedTempSource.tempName,
  }
  setGraphData()
}
watch(chosenTemp, () => {
  selectedTempSource = getCurrentTempSource(chosenTemp.value?.deviceUID, chosenTemp.value?.tempName)
  setGraphData()
  controlGraph.value?.setOption(option)
})

watch(currentDeviceStatus, () => {
  if (selectedTempSource == null) {
    return
  }
  const tempValue: string | undefined = deviceStore.currentDeviceStatus
      .get(selectedTempSource.deviceUID)
      ?.get(selectedTempSource.tempName)
      ?.temp
  if (tempValue == null) {
    return
  }
  selectedTempSourceTemp.value = Number(tempValue)
  tempLineData[0].value = [selectedTempSourceTemp.value, dutyMin]
  tempLineData[1].value = [selectedTempSourceTemp.value, dutyMax]
  // todo: there is a strange error only on the first time once switches back to a graph profile: Unknown series error
  controlGraph.value?.setOption({series: {id: 'tempLine', data: tempLineData}})
})

watch(settingsStore.allUIDeviceSettings, () => {
  // update all temp sources:
  fillTempSources()
  selectedTempSource = getCurrentTempSource(chosenTemp.value?.deviceUID, chosenTemp.value?.tempName)
  if (selectedTempSource == null) {
    return
  }
  // @ts-ignore
  option.series[1].lineStyle.color = selectedTempSource.color
  controlGraph.value?.setOption({series: {id: 'tempLine', lineStyle: {color: selectedTempSource?.color}}})
})

const controlPointMotionForTempX = (posX: number, selectedPointIndex: number): void => {
  if (selectedPointIndex === 0) {
    return // starting point is horizontally fixed
  }
  // We use 1 whole degree of separation between points so point index works perfect:
  const minActivePosition = selectedTempSource!.tempMin + selectedPointIndex
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
  controlPointMotionForTempX(posXY[0], dataIndex)
  controlPointMotionForDutyY(posXY[1], dataIndex)
  controlGraph.value?.setOption({
    series: [{id: 'a', data: data}],
    graphic: data
        .slice(0, data.length - 1) // no graphic for ending point
        .map((item, dataIndex) => ({
              id: dataIndex,
              type: 'circle',
              position: controlGraph.value?.convertToPixel('grid', item.value),
            })
        ),
  })
}

const showTooltip = (dataIndex: number): void => {
  controlGraph.value?.dispatchAction({
    type: 'showTip',
    seriesIndex: 0,
    dataIndex: dataIndex
  })
}

const hideTooltip = (): void => {
  controlGraph.value?.dispatchAction({
    type: 'hideTip'
  })
}

const createWatcherOfTempDutyText = (): WatchStopHandle =>
    watch([selectedTemp, selectedDuty], (newTempAndDuty) => {
      if (selectedPointIndex.value == null) {
        return
      }
      controlPointMotionForTempX(newTempAndDuty[0]!, selectedPointIndex.value)
      controlPointMotionForDutyY(newTempAndDuty[1]!, selectedPointIndex.value)
      data.slice(0, data.length - 1) // no graphic for ending point
          .forEach((pointData, dataIndex) =>
              // @ts-ignore
              graphicData[dataIndex].position = controlGraph.value?.convertToPixel('grid', pointData.value)
          )
      controlGraph.value?.setOption({
        series: [{id: 'a', data: data}],
        graphic: graphicData
      })
    }, {flush: 'post'})
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
        r: selectedSymbolSize / 2
      },
      cursor: 'pointer',
      silent: false,
      invisible: true,
      draggable: true,
      ondrag: function (eChartEvent: any) {
        if (eChartEvent?.event?.buttons != 1) {
          return // only apply on left button press
        }
        const posXY = controlGraph.value
            ?.convertFromPixel('grid', [(this as any).x, (this as any).y]) as [number, number] ?? [0, 0]
        onPointDragging(dataIndex, posXY)
        setTempAndDutyValues(dataIndex)
        showTooltip(dataIndex)
      },
      ondragend: function (eChartEvent: any) {
        if (eChartEvent?.event?.buttons != 1) {
          return // only apply on left button press
        }
        setTempAndDutyValues(dataIndex)
        tempDutyTextWatchStopper() // make sure we stop and runny watchers before changing the reference
        tempDutyTextWatchStopper = createWatcherOfTempDutyText()
        hideTooltip()
      },
      onmousemove: function (eChartEvent: any) {
        if (eChartEvent?.event?.buttons > 1) {
          return // only react with left mouse button or no button pressed - ignore event when right button pressed
        }
        tempDutyTextWatchStopper(); // unfortunately this also kills the numberInput if the cursor is left on top,
        // but due to the circular dependency of draggable points and the number inputs, this in not avoidable
        selectedPointIndex.value = dataIndex
        setTempAndDutyValues(dataIndex)
        showTooltip(dataIndex)
      },
      onmouseout: function () {
        setTempAndDutyValues(dataIndex)
        tempDutyTextWatchStopper()
        tempDutyTextWatchStopper = createWatcherOfTempDutyText()
        hideTooltip()
      },
      z: 100
    }
  }
  // clear and push
  graphicData.length = 0
  graphicData.push(
      ...data.slice(0, data.length - 1) // no graphic for ending point
          .map((item, dataIndex) => createGraphicDataForPoint(dataIndex, item.value))
  )
}

let draggableGraphicsCreated: boolean = false
const createDraggableGraphics = (): void => { // Add shadow circles (which is not visible) to enable drag.
  if (draggableGraphicsCreated) {
    return // we only need to do this once, AFTER the graph is drawn and visible
  }
  createGraphicDataFromPointData()
  controlGraph.value?.setOption({graphic: graphicData})
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
  const posXY = controlGraph.value
      ?.convertFromPixel('grid', [params.offsetX, params.offsetY]) as [number, number] ?? [0, 0];
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
    }
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
  showTooltip(indexToInsertAt)
  // this needs a bit of time for the graph to refresh before being set correctly:
  setTimeout(() => selectedPointIndex.value = indexToInsertAt, 50)
}

const deletePointFromLine = (params: any) => {
  if (params.componentType !== 'graphic' || params.event?.target?.id == null) {
    params.stop() // this stops any context menu from appearing in the graph
    return
  }
  params.event.stop()
  if (data.length <= selectedTempSource!.profileMinLength) {
    return
  }
  const dataIndexToRemove = params.event!.target!.id
  if (!(dataIndexToRemove > 0 && dataIndexToRemove < data.length - 1)) {
    return
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
  controlGraph.value?.setOption(option, {replaceMerge: ['series', 'graphic'], silent: true})
}

const showGraph = computed(() => {
  const shouldShow = selectedType.value != null
      && selectedType.value === ProfileType.Graph
      && chosenTemp.value != null
  if (shouldShow) {
    setTimeout(() => {
      const resizeObserver = new ResizeObserver((_) => {
        controlGraph.value?.setOption({
          graphic: data.map(function (item, dataIndex) {
            return {
              type: 'circle',
              position: controlGraph.value?.convertToPixel('grid', item.value)
            }
          })
        })
      })
      resizeObserver.observe(controlGraph.value?.$el)
      createDraggableGraphics() // we need to create AFTER the element is visible and rendered
    }, 500) // due to graph resizing, we really need a substantial delay on creation
  }
  return shouldShow;
})

const showDutyKnob = computed(() => {
  const shouldShow = selectedType.value != null && selectedType.value === ProfileType.Fixed
  if (shouldShow) {
    selectedDuty.value = currentProfile.value.speed_fixed ?? 50 // reasonable default if not already set
    selectedPointIndex.value = undefined // clear previous selected graph point
  }
  return shouldShow
})

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

const discardProfileState = () => {
  confirm.require({
    message: 'You are about to discard all changes made to the current profile. Are you sure?',
    header: 'Discard Changes?',
    icon: 'pi pi-exclamation-triangle',
    accept: () => {
      givenName.value = currentProfile.value.name
      selectedType.value = currentProfile.value.p_type
      selectedDuty.value = undefined
      selectedTemp.value = undefined
      selectedPointIndex.value = undefined
      selectedTempSource = getCurrentTempSource(
          currentProfile.value.temp_source?.device_uid,
          currentProfile.value.temp_source?.temp_name,
      )
      chosenTemp.value = undefined
      draggableGraphicsCreated = false
      hideTooltip()
      data.length = 0
      graphicData.length = 0
      controlGraph.value?.setOption(option, {notMerge: true})
      firstTimeChoosingTemp = true
      setTimeout(() => settingsChanged.value = false, 100)
    },
    reject: () => {
      // do nothing
    }
  })
}

const saveProfileState = () => {
  currentProfile.value.name = givenName.value
  currentProfile.value.p_type = selectedType.value
  if (currentProfile.value.p_type === ProfileType.Fixed) {
    currentProfile.value.speed_fixed = selectedDuty.value
    currentProfile.value.speed_profile.length = 0
    currentProfile.value.temp_source = undefined
  } else if (currentProfile.value.p_type === ProfileType.Graph && selectedTempSource != null) {
    const speedProfile: Array<[number, number]> = []
    for (const pointData of data) {
      speedProfile.push(pointData.value)
    }
    currentProfile.value.speed_profile = speedProfile
    currentProfile.value.temp_source = new ProfileTempSource(selectedTempSource.tempName, selectedTempSource.deviceUID)
    currentProfile.value.function_uid = chosenFunction.value.uid
    currentProfile.value.speed_fixed = undefined
  }
  settingsChanged.value = false // done editing
}

//----------------------------------------------------------------------------------------------------------------------
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
      series: [{id: 'a', data: data}],
    })
  })

  watch([givenName, selectedType, chosenTemp, selectedDuty], () => {
    settingsChanged.value = true
  })
  watch(settingsChanged, (newValue: boolean) => {
    emit('profileChange', newValue)
  })
})

</script>

<template>
  <div class="grid">
    <div class="col-fixed" style="width: 220px">
      <span class="p-float-label mt-2">
        <InputText id="name" v-model="givenName" class="w-full"/>
        <label for="name">Name</label>
      </span>
      <div class="p-float-label mt-4">
        <Dropdown v-model="selectedType" inputId="dd-profile-type" :options="profileTypes"
                  placeholder="Type" class="w-full"/>
        <label for="dd-profile-type">Type</label>
      </div>
      <div v-if="selectedType === ProfileType.Graph" class="p-float-label mt-4">
        <Dropdown v-model="chosenTemp" inputId="dd-temp-source" :options="tempSources" option-label="tempFrontendName"
                  option-group-label="deviceName" option-group-children="temps" placeholder="Temp Source"
                  class="w-full"/>
        <label for="dd-temp-source">Temp Source</label>
      </div>
      <div v-if="selectedType === ProfileType.Graph" class="p-float-label mt-4">
        <Dropdown v-model="chosenFunction" inputId="dd-function" :options="settingsStore.functions" option-label="name"
                  placeholder="Function" class="w-full"/>
        <label for="dd-function">Function</label>
      </div>
      <div class="align-content-end">
        <div v-if="selectedType === ProfileType.Fixed || selectedType === ProfileType.Graph" class="mt-6">
          <div v-if="selectedType === ProfileType.Graph" class="selected-point-wrapper">
            <label for="selected-point">For Selected Point:</label>
          </div>
          <InputNumber placeholder="Duty" v-model="selectedDuty" inputId="selected-duty" mode="decimal"
                       class="w-full" suffix="%" :input-style="{width: '58px'}"
                       showButtons :min="dutyMin" :max="dutyMax"
                       :disabled="selectedPointIndex == null && !showDutyKnob"/>
        </div>
        <div v-if="selectedType === ProfileType.Graph" class="mt-3">
          <InputNumber placeholder="Temp" v-model="selectedTemp" inputId="selected-temp" mode="decimal"
                       suffix="°" showButtons class="w-full" :disabled="!selectedPointIndex"
                       :min="inputNumberTempMin()" :max="inputNumberTempMax()"
                       buttonLayout="horizontal" :step="0.1" :input-style="{width: '55px'}"
                       incrementButtonIcon="pi pi-angle-right" decrementButtonIcon="pi pi-angle-left"/>
        </div>
        <div class="mt-6">
          <Button label="Apply" size="small" rounded @click="saveProfileState">
            <svg-icon class="p-button-icon p-button-icon-left pi" type="mdi" :path="mdiContentSaveMoveOutline"
                      size="1.35rem"/>
            <span class="p-button-label">Apply</span>
          </Button>
        </div>
      </div>
    </div>
    <div class="col">
      <Transition name="fade">
        <v-chart v-show="showGraph" class="control-graph" ref="controlGraph" :init-options="initOptions"
                 :option="option" :autoresize="true" :manual-update="true"
                 @contextmenu="deletePointFromLine" @zr:click="addPointToLine" @zr:contextmenu="deletePointFromLine"/>
      </Transition>
      <Transition name="fade">
        <Knob v-show="showDutyKnob" v-model="selectedDuty" valueTemplate="{value}%"
              :min="dutyMin" :max="dutyMax" :step="1" :size="400" class="text-center mt-8"
        />
      </Transition>
    </div>
  </div>
</template>

<style scoped lang="scss">
.control-graph {
  height: 56vh;
  width: 99.9%; // This handles an issue with the graph when the layout thinks it's too big for the container
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
  font-size: 12px;
  color: var(--cc-text-foreground);
}

</style>