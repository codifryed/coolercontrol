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
// As an alternative we could create a popup for all these controls, if it's going to be too small...

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const currentProfile = computed(() => settingsStore.profiles.find((profile) => profile.id === props.profileId))
const colors = useThemeColorsStore()
// @ts-ignore
const selectedType: Ref<ProfileType | undefined> = ref(ProfileType[currentProfile.value?.type] as ProfileType)
const profileTypes = Object.keys(ProfileType).filter(k => isNaN(Number(k)))
const givenName: Ref<string | undefined> = ref(currentProfile.value?.name)


interface AvailableTempSource {
  deviceUID: string
  tempName: string
  tempExternalName: string
  profileMinLength: number
  profileMaxLength: number
  tempMin: number
  tempMax: number
}

const tempSources: Array<AvailableTempSource> = []
for (const device of deviceStore.allDevices()) {
  for (const temp of device.status.temps) {
    if (device.info != null) {
      tempSources.push({
        deviceUID: device.uid,
        tempName: temp.name,
        tempExternalName: temp.external_name,
        profileMinLength: device.info.profile_min_length,
        profileMaxLength: device.info.profile_max_length,
        tempMin: device.info.temp_min,
        tempMax: device.info.temp_max,
      })
    }
  }
}
const associatedTempSource = tempSources.find((ts) =>
    ts.deviceUID === currentProfile.value?.temp_source?.device_uid
    && ts.tempName === currentProfile.value?.temp_source?.temp_name
)
const selectedTempSource: Ref<AvailableTempSource | undefined> = ref(associatedTempSource)
const selectedTemp: Ref<number | undefined> = ref()
const selectedDuty: Ref<number | undefined> = ref()
const selectedPointIndex: Ref<number | undefined> = ref()

// watch(props, () => {// watch for selected profile change
//   // todo: due to the addition of a key to the component, there is a new component created per profileId
//   // @ts-ignore
//   selectedType.value = ProfileType[currentProfile.value?.type] as ProfileType
//   givenName.value = currentProfile.value?.name
//   selectedTempSource.value = tempSources.find((ts) =>
//       ts.deviceUID === currentProfile.value?.temp_source?.device_uid
//       && ts.tempName === currentProfile.value?.temp_source?.temp_name
//   )
//   // todo: pop-up when there are changes to the props.profileId -> to either discard or save the changes made
// })

//------------------------------------------------------------------------------------------------------------------------------------------
// User Control Graph
// todo: function to create default data values and length
const data: Array<Array<number>> = [
  [0, 0],
  [33, 33],
  [50, 50],
  [66, 66],
  [100, 100],
]

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
    min: selectedTempSource.value?.tempMin,
    max: selectedTempSource.value?.tempMax,
    type: 'value',
    axisLabel: {
      formatter: '{value}°'
    },
    axisLine: {
      onZero: false,
      lineStyle: {
        color: '#c3ccdf',
        width: 1,
      }
    },
    splitLine: {
      lineStyle: {
        color: ['#4f5b6e'],
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
        color: '#c3ccdf',
        width: 1,
      }
    },
    splitLine: {
      lineStyle: {
        color: ['#4f5b6e'],
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
      symbolSize: 10,
      lineStyle: {
        width: 2,
        type: 'solid',
      },
      itemStyle: {},
      color: colors.themeColors().green,
      data: data
    }
  ],
  animation: true, // just causes unnecessary lag & artifacts
  animationDurationUpdate: 100,
}

watch(selectedTempSource, () => {
  // todo: move end points new min/max positions
  option.xAxis.min = selectedTempSource.value?.tempMin
  option.xAxis.max = selectedTempSource.value?.tempMax
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

  // Add shadow circles (which is not visible) to enable drag.
  // todo: if we set display:none on first load, this convert function won't work until the canvas has actually been drawn
  //  solution: use a reference var for the display/show logic, pass it to the v-chart component and use it to trigger
  //    the following creation of the draggable circles
  controlGraph.value?.setOption({
    graphic: data
        .slice(0, data.length > 1 ? data.length - 1 : 1) // no graphic for ending point
        .map(function (item, dataIndex) {
          // dataIndex = dataIndex + 1 // needed because of the above slicing
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
      && ProfileType[selectedType.value] !== ProfileType.DEFAULT
      && selectedTempSource.value != null
  if (shouldShow) {
    setTimeout(createDraggableGraphics, 100) // we need to create AFTER the element is visible and rendered
  }
  return shouldShow;
})

//----------------------------------------------------------------------------------------------------------------------
onMounted(async () => {
  controlGraph.value?.setOption(option)
  const resizeObserver = new ResizeObserver((_) => {
    controlGraph.value?.resize()
    updatePosition()
  })
  resizeObserver.observe(controlGraph.value?.$el)

  function updatePosition() {
    controlGraph.value?.setOption({
      graphic: data.map(function (item, dataIndex) {
        return {
          type: 'circle',
          position: controlGraph.value?.convertToPixel('grid', item)
        }
      })
    })
  }

  // Make sure on selected Point change, that there is only one.
  watch(selectedPointIndex, (dataIndex: number) => {
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
        <Dropdown v-model="selectedTempSource" inputId="dd-temp-source" :options="tempSources"
                  option-label="tempExternalName"
                  :disabled="(selectedType == null || ProfileType[selectedType] === ProfileType.DEFAULT)"
                  placeholder="Source" class="w-full md:w-14rem"/>
        <label for="dd-temp-source">Source</label>
      </div>
      <!--      todo: function-->
    </div>
    <div class="flex-grow-1">
      <Transition name="fade">
        <div class="grid" v-show="showGraph">
          <div class="col-12">
            <div class="control-graph">
              <v-chart ref="controlGraph" :init-options="initOptions" autoresize/>
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
            <Button label="Discard" size="small"/>
            <Button label="Apply" class="ml-3 mr-3" size="small"/>
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped lang="scss">
.control-graph {
  height: 50vh;
}

.fade-enter-active,
.fade-leave-active {
  transition: all 0.5s ease;
}

.fade-enter-from,
.fade-leave-to {
  height: 0;
  opacity: 0;
}
</style>