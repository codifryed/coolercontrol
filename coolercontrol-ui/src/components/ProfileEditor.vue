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
import {ProfileType} from "@/models/Profile"
import Button from 'primevue/button'
import Dropdown from 'primevue/dropdown'
import {computed, onMounted, type Ref, ref, watch, type WatchStopHandle} from "vue";
import InputText from 'primevue/inputtext'
import InputNumber from 'primevue/inputnumber'
import {useDeviceStore} from "@/stores/DeviceStore"
import * as echarts from 'echarts/core'
import {
  GraphicComponent,
  GridComponent,
  TooltipComponent,
  MarkAreaComponent,
} from 'echarts/components'
import {LineChart} from 'echarts/charts'
import {UniversalTransition} from 'echarts/features'
import {CanvasRenderer} from 'echarts/renderers'
import VChart from 'vue-echarts'
import {type EChartsOption} from "echarts";
import {useThemeColorsStore} from "@/stores/ThemeColorsStore";

echarts.use([
  GridComponent, LineChart, CanvasRenderer, UniversalTransition, TooltipComponent, GraphicComponent, MarkAreaComponent
])


interface Props {
  profileId: number
}

const props = defineProps<Props>()
const emit = defineEmits<{
  profileChange: []
}>()
// As an alternative we could create a popup for all these controls, if it's going to be too small...

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const currentProfile = computed(() => settingsStore.profiles.find((profile) => profile.id === props.profileId)!)
const colors = useThemeColorsStore()
// @ts-ignore
const selectedType: Ref<ProfileType | undefined> = ref(ProfileType[currentProfile.value.type] as ProfileType)
const profileTypes = Object.keys(ProfileType).filter(k => isNaN(Number(k)))
const givenName: Ref<string | undefined> = ref(currentProfile.value.name)
const speedProfile: Ref<Array<[number, number]>> = ref(currentProfile.value.speed_profile)
const speedDuty: Ref<number | undefined> = ref(currentProfile.value.speed_duty)

interface AvailableTemp {
  deviceUID: string // needed here as well for the dropdown selector
  tempName: string
  tempExternalName: string
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
  tempExternalName: string
}

const tempSources: Array<AvailableTempSources> = []
for (const device of deviceStore.allDevices()) {
  if (device.status.temps.length === 0 || device.info == null) {
    continue
  }
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
    deviceSource.temps.push({
      deviceUID: device.uid,
      tempName: temp.name,
      tempExternalName: deviceStore.toTitleCase(temp.name), // since we have grouping, we can have nicer names
    })
  }
  tempSources.push(deviceSource)
}
const getCurrentTempSource = (deviceUID: string | undefined, tempName: string | undefined): CurrentTempSource | undefined => {
  if (deviceUID == null || tempName == null) {
    return undefined
  }
  const tmpDevice = tempSources.find((ts) => ts.deviceUID === deviceUID)
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
      tempExternalName: tmpTemp.tempExternalName,
    }
  }
  return undefined
}
let selectedTempSource: CurrentTempSource | undefined = getCurrentTempSource(
    currentProfile.value.temp_source?.device_uid,
    currentProfile.value.temp_source?.temp_name,
)

const chosenTemp: Ref<AvailableTemp | undefined> = ref()
const selectedTemp: Ref<number | undefined> = ref()
const selectedDuty: Ref<number | undefined> = ref()
const selectedPointIndex: Ref<number | undefined> = ref()
const settingsChanged: Ref<boolean> = ref(false)

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

const lineSpace = (startValue: number, stopValue: number, cardinality: number, precision: number) => {
  const arr = []
  const step = (stopValue - startValue) / (cardinality - 1)
  for (let i = 0; i < cardinality; i++) {
    const value = startValue + (step * i)
    const precisionValue = precision > 0 ? 10 * precision : 1
    arr.push(Math.round(value * precisionValue) / precisionValue)
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
if (speedProfile.value.length > 2 && selectedTempSource != null) {
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

const markAreaData: [({ xAxis: number })[], ({ xAxis: number })[]] = [
  [{xAxis: axisXTempMin}, {xAxis: axisXTempMin}],
  [{xAxis: axisXTempMax}, {xAxis: axisXTempMax}]
]

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
    }
  ],
  animation: true,
  animationDuration: 500,
  animationDurationUpdate: 0,
}

watch(chosenTemp, () => {
  selectedTempSource = getCurrentTempSource(chosenTemp.value?.deviceUID, chosenTemp.value?.tempName)
  if (firstTimeChoosingTemp) {
    data.length = 0
    data.push(...defaultDataValues())
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
  controlGraph.value?.setOption({
    series: [
      {
        id: 'a',
        data: data,
        markArea: {
          data: markAreaData
        }
      }
    ],
  })
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

const controlGraph = ref<InstanceType<typeof VChart> | null>(null)

let draggableGraphicsCreated: boolean = false
const createDraggableGraphics = (): void => {
  if (draggableGraphicsCreated) {
    return // we only need to do this once, AFTER the graph is drawn and visible
  }
  const createWatcherOfTempDutyText = (): WatchStopHandle =>
      watch([selectedTemp, selectedDuty], (newTempAndDuty) => {
        // todo: point limitations must also be taken into consideration here
        // @ts-ignore
        data[selectedPointIndex.value].value = newTempAndDuty
        controlGraph.value?.setOption({
          series: [
            {
              id: 'a',
              data: data
            }
          ],
          graphic: [
            {
              id: selectedPointIndex.value,
              type: 'circle',
              // @ts-ignore
              position: controlGraph.value?.convertToPixel('grid', newTempAndDuty),
            }
          ]
        })
      }, {flush: 'post'})
  let tempDutyTextWatchStopper = createWatcherOfTempDutyText()

  controlGraph.value?.setOption({ // Add shadow circles (which is not visible) to enable drag.
    graphic: data
        .slice(0, data.length > 1 ? data.length - 1 : 1) // no graphic for ending point
        .map((item, dataIndex) => {
          return {
            id: dataIndex,
            type: 'circle',
            position: controlGraph.value?.convertToPixel('grid', item.value),
            shape: {
              cx: 0,
              cy: 0,
              r: selectedSymbolSize / 2
            },
            cursor: 'pointer',
            silent: false,
            invisible: true,
            draggable: true,
            onmousedown: function () {
              selectedPointIndex.value = dataIndex
              setTempAndDutyValues(dataIndex)
            },
            ondrag: function (dx: number, dy: number) {
              const posXY = controlGraph.value
                  ?.convertFromPixel('grid', [(this as any).x, (this as any).y]) as [number, number] ?? [0, 0]
              onPointDragging(dataIndex, posXY)
              setTempAndDutyValues(dataIndex)
              showTooltip(dataIndex)
            },
            ondragend: function () {
              setTempAndDutyValues(dataIndex)
              tempDutyTextWatchStopper() // make sure we stop and runny watchers before changing the reference
              tempDutyTextWatchStopper = createWatcherOfTempDutyText()
              hideTooltip(dataIndex)
            },
            onmousemove: function () {
              tempDutyTextWatchStopper() // unfortunately this also kills the numberInput if the cursor is left on top,
              // but due to the circular dependency of draggable points and the number inputs, this in not avoidable
              selectedPointIndex.value = dataIndex
              setTempAndDutyValues(dataIndex)
              showTooltip(dataIndex)
            },
            onmouseout: function () {
              setTempAndDutyValues(dataIndex)
              tempDutyTextWatchStopper()
              tempDutyTextWatchStopper = createWatcherOfTempDutyText()
              hideTooltip(dataIndex)
            },
            z: 100
          }
        })
  })

  function setTempAndDutyValues(dataIndex: number) {
    selectedTemp.value = Math.round(data[dataIndex].value[0] * 10) / 10
    selectedDuty.value = Math.round(data[dataIndex].value[1])
  }

  function onPointDragging(dataIndex: number, posXY: [number, number]) {
    controlPointMotionForTempX(posXY[0], dataIndex)
    controlPointMotionForDutyY(posXY[1], dataIndex)
    controlGraph.value?.setOption({
      series: [
        {
          id: 'a',
          data: data
        }
      ],
      graphic: data
          .slice(0, data.length > 1 ? data.length - 1 : 1) // no graphic for ending point
          .map(function (item, dataIndex) {
            return {
              id: dataIndex,
              type: 'circle',
              position: controlGraph.value?.convertToPixel('grid', item.value),
            }
          }),
    })
  }

  function showTooltip(dataIndex: number) {
    controlGraph.value?.dispatchAction({
      type: 'showTip',
      seriesIndex: 0,
      dataIndex: dataIndex
    })
  }

  function hideTooltip(dataIndex: number) {
    controlGraph.value?.dispatchAction({
      type: 'hideTip'
    })
  }
}

const showGraph = computed(() => {
  const shouldShow = selectedType.value != null
      // @ts-ignore
      && ProfileType[selectedType.value] === ProfileType.GRAPH
      && chosenTemp.value != null
  if (shouldShow) {
    setTimeout(() => {
      controlGraph.value?.setOption(option)
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
      series: [{
        id: 'a',
        data: data
      }],
    })
  })

  watch([givenName, selectedType, chosenTemp, speedProfile], () => {
    settingsChanged.value = true
    emit('profileChange')
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
      <div class="p-float-label mt-5">
        <Dropdown v-model="selectedType" inputId="dd-profile-type" :options="profileTypes"
                  placeholder="Type" class="w-full"/>
        <label for="dd-profile-type">Type</label>
      </div>
      <div class="p-float-label mt-5">
        <Dropdown v-model="chosenTemp" inputId="dd-temp-source" :options="tempSources"
                  option-label="tempExternalName" option-group-label="deviceName" option-group-children="temps"
                  :disabled="(selectedType == null || ProfileType[selectedType] === ProfileType.DEFAULT)"
                  placeholder="Temp Source" class="w-full"/>
        <label for="dd-temp-source">Temp Source</label>
      </div>
      <!--      todo: function-->
      <div class="align-content-end">
        <div class="mt-8">
          <InputNumber placeholder="Duty" v-model="selectedDuty" inputId="selected-duty" mode="decimal"
                       class="w-full" suffix="%" :input-style="{width: '58px'}"
                       showButtons :min="dutyMin" :max="dutyMax" :disabled="selectedPointIndex == null"/>
        </div>
        <div class="mt-3">
          <InputNumber placeholder="Temp" v-model="selectedTemp" inputId="selected-temp" mode="decimal"
                       suffix="°" showButtons :min="selectedTempSource?.tempMin ?? 0" class="w-full"
                       :max="selectedTempSource?.tempMax ?? 100" :disabled="!selectedPointIndex"
                       buttonLayout="horizontal" :step="0.1" :input-style="{width: '55px'}"
                       incrementButtonIcon="pi pi-angle-right" decrementButtonIcon="pi pi-angle-left"/>
        </div>
        <div class="mt-8">
          <Button icon="pi pi-fw pi-times" icon-pos="right" label="Discard" size="small" class="w-full" rounded
                  :disabled="!settingsChanged"/>
        </div>
        <div class="mt-3">
          <Button icon="pi pi-fw pi-check" icon-pos="right" label="Apply" size="small" class="w-full" rounded
                  :disabled="!settingsChanged"/>
        </div>
      </div>
    </div>
    <div class="col">
      <Transition name="fade">
        <div class="control-graph" v-show="showGraph">
          <v-chart v-if="showGraph" ref="controlGraph" :init-options="initOptions" autoresize/>
        </div>
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
  transition: all 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  height: 0;
  opacity: 0;
}
</style>