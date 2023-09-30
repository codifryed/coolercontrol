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
  DataZoomComponent,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  TitleComponent,
  TooltipComponent
} from 'echarts/components'
import {LineChart} from 'echarts/charts'
import {UniversalTransition} from 'echarts/features'
import {CanvasRenderer} from 'echarts/renderers'
import VChart from 'vue-echarts'
import {type EChartsOption} from "echarts";
import {useThemeColorsStore} from "@/stores/ThemeColorsStore";

echarts.use([
  GridComponent, LineChart, CanvasRenderer, UniversalTransition, TitleComponent, TooltipComponent, LegendComponent, DataZoomComponent,
  GraphicComponent
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

const defaultDataValues = (): Array<[number, number]> => {
  const result: Array<[number, number]> = []
  if (selectedTempSource != null) {
    const profileLength = selectedTempSource.profileMinLength <= 5 && selectedTempSource.profileMaxLength >= 5
        ? 5 : selectedTempSource.profileMaxLength;
    const temps = lineSpace(selectedTempSource.tempMin, selectedTempSource.tempMax, profileLength, 1)
    const duties = lineSpace(0, 100, profileLength, 0)
    for (const [index, temp] of temps.entries()) {
      result.push([temp, duties[index]])
    }
  } else {
    for (let i = 0; i < 100; i = i + 25) {
      const value = 25 * i
      result.push([value, value])
    }
  }
  return result
}

const data: Array<Array<number>> = []
if (speedProfile.value.length > 2 && selectedTempSource != null) {
  data.push(...speedProfile.value)
} else {
  data.push(...defaultDataValues())
}

const initOptions = {
  useDirtyRect: true,
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
          params.data[1].toFixed(0) + '% ' +
          params.data[0].toFixed(1) + '°'
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
    min: selectedTempSource?.tempMin,
    max: selectedTempSource?.tempMax,
    type: 'value',
    axisLabel: {
      formatter: '{value}°'
    },
    axisLine: {
      onZero: false,
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
    min: 0,
    max: 100,
    type: 'value',
    axisLabel: {
      formatter: '{value}%'
    },
    axisLine: {
      onZero: false,
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
        disabled: true,
      },
      data: data
    }
  ],
  animation: true,
  animationDuration: 300,
  animationDurationUpdate: 100,
}

watch(chosenTemp, () => {
  selectedTempSource = getCurrentTempSource(chosenTemp.value?.deviceUID, chosenTemp.value?.tempName)
  data.length = 0
  data.push(...defaultDataValues()) // todo: instead of resetting the graph, re-scale to match the new temp range
  // @ts-ignore
  option.xAxis!.min = selectedTempSource?.tempMin
  // @ts-ignore
  option.xAxis!.max = selectedTempSource?.tempMax
  controlGraph.value?.setOption(option)
})

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
        data[selectedPointIndex.value] = newTempAndDuty
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
            position: controlGraph.value?.convertToPixel('grid', item),
            shape: {
              cx: 0,
              cy: 0,
              r: 20 / 2 // symbolsize / 2
            },
            cursor: 'pointer',
            style: {
              fill: colors.themeColors().green,
            },
            silent: false,
            invisible: true,
            draggable: true,
            onmousedown: function () {
              const posXY = controlGraph.value?.convertFromPixel('grid', [(this as any).x, (this as any).y]) ?? [0, 0]
              setTempAndDutyValues(dataIndex, posXY)
            },
            onmouseup: function () {
              const posXY = controlGraph.value?.convertFromPixel('grid', [(this as any).x, (this as any).y]) ?? [0, 0]
              setTempAndDutyValues(dataIndex, posXY)
              tempDutyTextWatchStopper() // make sure we stop and runny watchers before changing the reference
              tempDutyTextWatchStopper = createWatcherOfTempDutyText()
            },
            ondrag: function (dx: number, dy: number) {
              const [posX, posY] = controlGraph.value?.convertFromPixel('grid', [(this as any).x, (this as any).y]) ?? [0, 0]
              if (posX < 0 || posX > 100 || posY < 0 || posY > 100) {
                const clampedX = Math.min(Math.max(posX, 0), 100)
                const clampedY = Math.min(Math.max(posY, 0), 100)
                this.position = controlGraph.value?.convertToPixel('grid', [clampedX, clampedY])
                return // don't allow the point to go outside the grid
              }
              onPointDragging(dataIndex, [posX, posY])
            },
            onmousemove: function () {
              const [posX, posY] = controlGraph.value?.convertFromPixel('grid', [(this as any).x, (this as any).y]) ?? [0, 0]
              tempDutyTextWatchStopper()
              setTempAndDutyValues(dataIndex, [posX, posY])
              showTooltip(dataIndex)
            },
            onmouseout: function () {
              const posXY = controlGraph.value?.convertFromPixel('grid', [(this as any).x, (this as any).y]) ?? [0, 0]
              setTempAndDutyValues(dataIndex, posXY)
              tempDutyTextWatchStopper() // make sure we stop and runny watchers before changing the reference
              tempDutyTextWatchStopper = createWatcherOfTempDutyText()
              hideTooltip(dataIndex)
            },
            z: 100
          }
        })
  })

  function setTempAndDutyValues(dataIndex: number, posXY: number[]) {
    selectedPointIndex.value = dataIndex
    // @ts-ignore
    selectedTemp.value = Number(Math.round(posXY[0] + 'e1') + 'e-1')
    // @ts-ignore
    selectedDuty.value = Number(Math.round(posXY[1] + 'e0') + 'e-0')
  }

  function onPointDragging(dataIndex: number, posXY: number[]) {
    data[dataIndex] = posXY
    let maxX = 0
    let maxY = 0
    // todo: handle logic that holds the points in reasonable position
    for (const [index, [dataX, dataY]] of data.entries()) {
      if (dataX >= maxX) {
        maxX = dataX
      } else {
        data[index][0] = maxX
      }
      if (dataY >= maxY) {
        maxY = dataY
      } else {
        data[index][1] = maxY
      }
    }
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
            // dataIndex = dataIndex + 1 // needed because of the above slicing
            return {
              id: dataIndex,
              type: 'circle',
              position: controlGraph.value?.convertToPixel('grid', item),
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
              position: controlGraph.value?.convertToPixel('grid', item)
            }
          })
        })
      })
      resizeObserver.observe(controlGraph.value?.$el)
      createDraggableGraphics() // we need to create AFTER the element is visible and rendered
    }, 100)
  }
  return shouldShow;
})

//----------------------------------------------------------------------------------------------------------------------
onMounted(async () => {
  // Make sure on selected Point change, that there is only one.
  watch(selectedPointIndex, (dataIndex) => {
    const graphicCircles = []
    for (let i = 0; i < data.length - 1; i++) {
      graphicCircles.push({
        id: i,
        type: 'circle',
        invisible: dataIndex != i,
      })
    }
    controlGraph.value?.setOption({
      graphic: graphicCircles
    })
  })

  watch([givenName, selectedType, chosenTemp, speedProfile], () => {
    settingsChanged.value = true
    emit('profileChange')
  })
})

</script>

<template>
  <div class="flex main-grid">
    <div class="">
      <span class="p-float-label mt-2">
        <InputText id="name" v-model="givenName"/>
        <label for="name">Name</label>
      </span>
      <div class="p-float-label mt-5">
        <Dropdown v-model="selectedType" inputId="dd-profile-type" :options="profileTypes"
                  placeholder="Type" class="w-full md:w-14rem"/>
        <label for="dd-profile-type">Type</label>
      </div>
      <div class="p-float-label mt-5">
        <Dropdown v-model="chosenTemp" inputId="dd-temp-source" :options="tempSources" filter
                  option-label="tempExternalName" option-group-label="deviceName" option-group-children="temps"
                  :disabled="(selectedType == null || ProfileType[selectedType] === ProfileType.DEFAULT)"
                  placeholder="Temp Source" class="w-full md:w-14rem"/>
        <label for="dd-temp-source">Temp Source</label>
      </div>
      <!--      todo: function-->
    </div>
    <div class="flex-grow-1">
      <Transition name="fade">
        <div class="grid" v-show="showGraph">
          <div class="col-12">
            <div class="control-graph">
              <v-chart v-if="showGraph" ref="controlGraph" :init-options="initOptions" autoresize/>
            </div>
          </div>
          <div class="col-6">
            <InputNumber placeholder="Duty" v-model="selectedDuty" inputId="selected-duty" mode="decimal"
                         suffix="%"
                         showButtons :min="0" :max="100" :disabled="selectedPointIndex == null"
                         :input-style="{width: '58px'}"
                         class="ml-4 mr-3"
            />
            <InputNumber placeholder="Temp" v-model="selectedTemp" inputId="selected-temp" mode="decimal"
                         suffix="°"
                         showButtons :min="0" :max="100" :disabled="!selectedPointIndex"
                         buttonLayout="horizontal"
                         incrementButtonIcon="pi pi-angle-right" decrementButtonIcon="pi pi-angle-left"
                         :step="0.1"
                         :input-style="{width: '55px'}"
            />
          </div>
          <div class="col-6 text-right">
            <!--          todo: onclick actions for both buttons-->
            <Button icon="pi pi-times" label="Discard" size="small" :disabled="!settingsChanged"/>
            <Button icon="pi pi-check" label="Apply" class="ml-3 mr-3" size="small" :disabled="!settingsChanged"/>
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped lang="scss">
.control-graph {
  height: 60vh;
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