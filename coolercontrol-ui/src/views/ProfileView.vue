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
import { mdiContentSaveOutline, mdiInformationSlabCircleOutline, mdiMemory } from '@mdi/js'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import {
    Function,
    FunctionType,
    Profile,
    ProfileMixFunctionType,
    ProfileTempSource,
    ProfileType,
    getProfileTypeDisplayName,
    getProfileMixFunctionTypeDisplayName,
} from '@/models/Profile.ts'
import Button from 'primevue/button'
import MultiSelect from 'primevue/multiselect'
import {
    computed,
    inject,
    nextTick,
    onMounted,
    onUnmounted,
    ref,
    type Ref,
    watch,
    toRaw,
    type WatchStopHandle,
} from 'vue'
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
    TitleComponent,
} from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { UniversalTransition } from 'echarts/features'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import type { GraphicComponentLooseOption } from 'echarts/types/dist/shared.d.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { useToast } from 'primevue/usetoast'
import { $enum } from 'ts-enum-util'
import MixProfileEditorChart from '@/components/MixProfileEditorChart.vue'
import Select from 'primevue/select'
import { onBeforeRouteLeave, onBeforeRouteUpdate } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'
import _ from 'lodash'
import { useI18n } from 'vue-i18n'
import OverlayProfileEditorChart from '@/components/OverlayProfileEditorChart.vue'
import EntityTitleRename from '@/components/EntityTitleRename.vue'
import { Emitter, EventType } from 'mitt'

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
    profileUID: string
}

const props = defineProps<Props>()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const deviceStore = useDeviceStore()
// We need to use the raw state to watch for changes, as the pinia reactive proxy isn't properly
// reacting to changes from Vue's shallowRef & triggerRef anymore.
const rawStore = toRaw(deviceStore.$state)
const settingsStore = useSettingsStore()
const colors = useThemeColorsStore()
const toast = useToast()
const confirm = useConfirm()
const { t } = useI18n()

const contextIsDirty: Ref<boolean> = ref(false)
const tableDataKey: Ref<number> = ref(0)

const currentProfile = computed(
    () => settingsStore.profiles.find((profile) => profile.uid === props.profileUID)!,
)
const selectedType: Ref<ProfileType> = ref(currentProfile.value.p_type)
const profileTypeOptions = computed(() => {
    return [...$enum(ProfileType).values()].map((type) => ({
        value: type,
        label: getProfileTypeDisplayName(type),
    }))
})
const mixFunctionTypeOptions = computed(() => {
    return [...$enum(ProfileMixFunctionType).values()].map((type) => ({
        value: type,
        label: getProfileMixFunctionTypeDisplayName(type),
    }))
})
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
    settingsStore.profiles.filter((profile) => {
        if (profile.uid === props.profileUID) return false
        if (profile.p_type === ProfileType.Graph) return true
        if (profile.p_type !== ProfileType.Mix) return false
        // Exclude Mix profiles that already have Mix sub-members (can't be a child if already a parent)
        const hasMixSubMembers = profile.member_profile_uids.some(
            (uid) => settingsStore.profiles.find((p) => p.uid === uid)?.p_type === ProfileType.Mix,
        )
        if (hasMixSubMembers) return false
        // Exclude if circular reference (member contains current profile)
        if (profile.member_profile_uids.includes(props.profileUID)) return false
        // Exclude if current profile is already a child of another Mix (can't become a parent)
        const currentIsChildOfAnotherMix = settingsStore.profiles.some(
            (p) =>
                p.p_type === ProfileType.Mix &&
                p.uid !== props.profileUID &&
                p.member_profile_uids.includes(props.profileUID),
        )
        if (currentIsChildOfAnotherMix) return false
        return true
    }),
)
const offsetMemberProfileOptions: Ref<Array<Profile>> = computed(() =>
    settingsStore.profiles.filter(
        (profile) =>
            profile.uid !== props.profileUID &&
            (profile.p_type === ProfileType.Graph || profile.p_type === ProfileType.Mix),
    ),
)
const chosenMemberProfiles: Ref<Array<Profile>> = ref(
    currentProfile.value.member_profile_uids.map(
        (uid) => settingsStore.profiles.find((profile) => profile.uid === uid)!,
    ),
)
const chosenOverlayMemberProfile: Ref<Profile | undefined> = ref(
    currentProfile.value.member_profile_uids.length != 1
        ? undefined
        : settingsStore.profiles.find(
              (profile) => profile.uid === currentProfile.value.member_profile_uids[0],
          ),
)
const chosenProfileMixFunction: Ref<ProfileMixFunctionType> = ref(
    currentProfile.value.mix_function_type != null
        ? currentProfile.value.mix_function_type
        : ProfileMixFunctionType.Max,
)
const chosenOverlayOffsetType: Ref<string> = ref(
    currentProfile.value.offset_profile == null || currentProfile.value.offset_profile.length < 2
        ? 'static'
        : 'graph',
)
const overlayOffsetTypeOptions: Array<{ value: string; label: string }> = [
    {
        value: 'static',
        label: t('views.profiles.offsetTypeStatic'),
    },
    {
        value: 'graph',
        label: t('views.profiles.offsetTypeGraph'),
    },
]
const selectedTemp: Ref<number | undefined> = ref()
const selectedDuty: Ref<number | undefined> = ref()
const selectedStaticOffset: Ref<number | undefined> = ref()
const selectedGraphOffset: Ref<Array<[number, number]>> = ref([])
const selectedPointIndex: Ref<number | undefined> = ref()
const selectedTempSourceTemp: Ref<number | undefined> = ref()
const knobSize: Ref<number> = ref(100)
if (currentProfile.value.offset_profile != null && currentProfile.value.offset_profile.length > 1) {
    selectedGraphOffset.value = currentProfile.value.offset_profile
}

//------------------------------------------------------------------------------------------------------------------------------------------
// User Control Graph

const defaultSymbolSize: number = deviceStore.getREMSize(1.0)
const defaultSymbolColor: string = colors.themeColors.bg_two
const selectedSymbolSize: number = deviceStore.getREMSize(1.25)
const selectedSymbolColor: string = colors.themeColors.accent
const graphTempMinLimit: number = 0
const graphTempMaxLimit: number = 150
const axisXTempMin: Ref<number> = ref(currentProfile.value.temp_min ?? 0)
const axisXTempMax: Ref<number> = ref(currentProfile.value.temp_max ?? 100)
const dutyMin: number = 0
const dutyMax: number = 100
const offsetMin: number = -100
const offsetMax: number = 100
const MIN_TEMP_SEPARATION: number = 1.0 // Minimum temperature separation between adjacent points
let firstTimeChoosingTemp: boolean = true
const staticOffsetPrefix = computed(() =>
    selectedStaticOffset.value != null && selectedStaticOffset.value > 0 ? '+' : '',
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
            color: colors.themeColors.text_color_secondary,
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
            color: colors.themeColors.text_color_secondary,
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
    option.series[2].lineStyle.color = selectedTempSource.color
    // @ts-ignore
    option.series[2].markPoint.label.color = selectedTempSource.color
    // @ts-ignore
    option.series[2].markPoint.data[0].coord[0] = selectedTempSourceTemp.value
    // @ts-ignore
    option.series[2].markPoint.data[0].value = selectedTempSourceTemp.value
    tempLineData[0].value = [selectedTempSourceTemp.value!, dutyMin]
    tempLineData[1].value = [selectedTempSourceTemp.value!, dutyMax]
}
const setFunctionGraphData = (): void => {
    if (chosenFunction.value.f_type === FunctionType.Identity) {
        option.series[0].smooth = 0.0
        option.series[1].smooth = 0.0
        option.series[3].smooth = 0.0
        // @ts-ignore
        option.series[0].lineStyle.shadowColor = colors.themeColors.bg_one
        option.series[0].lineStyle.shadowBlur = 10
    } else {
        option.series[0].smooth = 0.1
        option.series[1].smooth = 0.1
        option.series[3].smooth = 0.1
        // @ts-ignore
        option.series[0].lineStyle.shadowColor = colors.themeColors.accent
        // size of the blur around the line:
        option.series[0].lineStyle.shadowBlur = 20
    }
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
    setFunctionGraphData()
}

const updateTemps = () => {
    for (const tempDevice of tempSources.value) {
        for (const availableTemp of tempDevice.temps) {
            availableTemp.temp =
                deviceStore.currentDeviceStatus
                    .get(availableTemp.deviceUID)!
                    .get(availableTemp.tempName)!.temp || '0.0'
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
watch(chosenFunction, () => {
    setFunctionGraphData()
    // needed as the graphics get a bit lost for some reason after ^:
    createGraphicDataFromPointData()
    controlGraph.value?.setOption(option)
})

watch(rawStore.currentDeviceStatus, () => {
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
    option.series[2].lineStyle.color = selectedTempSource.color
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
    contextIsDirty.value = true
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
    contextIsDirty.value = true
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
            { id: 'hit-area', data: data },
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
            { id: 'hit-area', data: data },
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
                    { id: 'hit-area', data: data },
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

    // Validate minimum temperature separation from adjacent points
    const prevPoint = data[indexToInsertAt - 1]
    const nextPoint = data[indexToInsertAt]
    // Ensure new point's duty is between previous and next point's duty
    posXY[1] = Math.max(posXY[1], prevPoint.value[1])
    posXY[1] = Math.min(posXY[1], nextPoint.value[1])

    // Calculate how much room we have to move each adjacent point
    let lowerMinTemp: number
    if (indexToInsertAt - 1 === 0) {
        lowerMinTemp = Math.max(selectedTempSource!.tempMin, axisXTempMin.value)
    } else {
        lowerMinTemp = data[indexToInsertAt - 2].value[0] + MIN_TEMP_SEPARATION
    }
    const lowerRoom = Math.max(0, prevPoint.value[0] - lowerMinTemp)

    let upperMaxTemp: number
    if (indexToInsertAt === data.length - 1) {
        upperMaxTemp = Math.min(selectedTempSource!.tempMax, axisXTempMax.value)
    } else {
        upperMaxTemp = data[indexToInsertAt + 1].value[0] - MIN_TEMP_SEPARATION
    }
    const upperRoom = Math.max(0, upperMaxTemp - nextPoint.value[0])

    // Calculate valid range for the new point
    const minValidPos = prevPoint.value[0] + MIN_TEMP_SEPARATION - lowerRoom
    const maxValidPos = nextPoint.value[0] - MIN_TEMP_SEPARATION + upperRoom

    // Check if there's any valid position for the new point
    if (minValidPos > maxValidPos) return // No room at all

    // Clamp click position to valid range
    posXY[0] = Math.max(minValidPos, Math.min(maxValidPos, posXY[0]))

    // Now adjust adjacent points as needed
    const gapToPrev = posXY[0] - prevPoint.value[0]
    const gapToNext = nextPoint.value[0] - posXY[0]

    if (gapToPrev < MIN_TEMP_SEPARATION) {
        prevPoint.value[0] = posXY[0] - MIN_TEMP_SEPARATION
    }
    if (gapToNext < MIN_TEMP_SEPARATION) {
        nextPoint.value[0] = posXY[0] + MIN_TEMP_SEPARATION
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
    tableDataKey.value++
    contextIsDirty.value = true
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
    tableDataKey.value++
    contextIsDirty.value = true
}

//--------------------------------------------------------------------------------------------------

// Points table position (local state, not persisted)
type TablePosition = 'top-left' | 'bottom-right'
const tablePosition: Ref<TablePosition> = ref('top-left')

const tablePositionClasses = computed(() => ({
    'top-16 left-[5.5rem]': tablePosition.value === 'top-left',
    'bottom-16 right-[7rem]': tablePosition.value === 'bottom-right',
}))

const cycleTablePosition = () => {
    tablePosition.value = tablePosition.value === 'top-left' ? 'bottom-right' : 'top-left'
}

const selectPointFromTable = (idx: number) => {
    tempDutyTextWatchStopper()
    selectedPointIndex.value = idx
    setTempAndDutyValues(idx)
}

// Calculate min/max temp for a specific point index (for table editing)
const getPointTempMin = (idx: number): number => {
    if (selectedTempSource == null) return axisXTempMin.value
    return Math.max(selectedTempSource.tempMin, axisXTempMin.value) + idx
}

const getPointTempMax = (idx: number): number => {
    if (selectedTempSource == null) return axisXTempMax.value
    if (idx === 0) return Math.max(selectedTempSource.tempMin, axisXTempMin.value)
    if (idx === data.length - 1) return Math.min(selectedTempSource.tempMax, axisXTempMax.value)
    return Math.min(selectedTempSource.tempMax, axisXTempMax.value) - (data.length - 1 - idx)
}

// Update point value from table (reuses existing constraint functions)
const updatePointFromTable = (idx: number, newTemp: number, newDuty: number): void => {
    selectPointFromTable(idx)
    controlPointMotionForTempX(newTemp, idx)
    controlPointMotionForDutyY(newDuty, idx)
    refreshGraphAfterTableEdit(idx)
}

const refreshGraphAfterTableEdit = (_idx?: number): void => {
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
const incrementPointTemp = (idx: number): void => {
    const newTemp = Math.min(data[idx].value[0] + 0.1, getPointTempMax(idx))
    updatePointFromTable(idx, newTemp, data[idx].value[1])
}

const decrementPointTemp = (idx: number): void => {
    const newTemp = Math.max(data[idx].value[0] - 0.1, getPointTempMin(idx))
    updatePointFromTable(idx, newTemp, data[idx].value[1])
}

const incrementPointDuty = (idx: number): void => {
    if (idx === data.length - 1) return // Last point duty is fixed
    const newDuty = Math.min(data[idx].value[1] + 1, dutyMax)
    updatePointFromTable(idx, data[idx].value[0], newDuty)
}

const decrementPointDuty = (idx: number): void => {
    if (idx === data.length - 1) return // Last point duty is fixed
    const newDuty = Math.max(data[idx].value[1] - 1, dutyMin)
    updatePointFromTable(idx, data[idx].value[0], newDuty)
}

// Scroll wheel handlers for table cells
const handleTempScroll = (event: WheelEvent, idx: number): void => {
    event.preventDefault()
    if (event.deltaY < 0) incrementPointTemp(idx)
    else decrementPointTemp(idx)
}

const handleDutyScroll = (event: WheelEvent, idx: number): void => {
    event.preventDefault()
    if (event.deltaY < 0) incrementPointDuty(idx)
    else decrementPointDuty(idx)
}

// Direct input handlers for table cells
const handleTempInput = (idx: number, value: number | null): void => {
    if (value == null || idx === 0 || idx === data.length - 1) return
    const clampedTemp = Math.max(getPointTempMin(idx), Math.min(value, getPointTempMax(idx)))
    updatePointFromTable(idx, clampedTemp, data[idx].value[1])
}

const handleDutyInput = (idx: number, value: number | null): void => {
    if (value == null || idx === data.length - 1) return
    const clampedDuty = Math.max(dutyMin, Math.min(value, dutyMax))
    updatePointFromTable(idx, data[idx].value[0], clampedDuty)
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
    if (data.length >= selectedTempSource!.profileMaxLength) return
    if (afterIdx >= data.length - 1) return // Can't add after last point

    // Calculate midpoint between current and next point
    const currentPoint = data[afterIdx].value
    const nextPoint = data[afterIdx + 1].value

    // Ensure minimum temperature separation
    const tempGap = nextPoint[0] - currentPoint[0]
    const requiredGap = MIN_TEMP_SEPARATION * 2

    // If gap is too small, try to make room by moving adjacent points
    if (tempGap < requiredGap) {
        const deficit = requiredGap - tempGap

        // Calculate how much room we have to move each point
        let lowerMinTemp: number
        if (afterIdx === 0) {
            // First point: constrained by temp source min
            lowerMinTemp = Math.max(selectedTempSource!.tempMin, axisXTempMin.value)
        } else {
            // Middle point: constrained by previous point
            lowerMinTemp = data[afterIdx - 1].value[0] + MIN_TEMP_SEPARATION
        }
        const lowerRoom = Math.max(0, currentPoint[0] - lowerMinTemp)

        let upperMaxTemp: number
        if (afterIdx + 1 === data.length - 1) {
            // Last point: constrained by temp source max
            upperMaxTemp = Math.min(selectedTempSource!.tempMax, axisXTempMax.value)
        } else {
            // Middle point: constrained by next point
            upperMaxTemp = data[afterIdx + 2].value[0] - MIN_TEMP_SEPARATION
        }
        const upperRoom = Math.max(0, upperMaxTemp - nextPoint[0])

        // Check if we have enough total room
        if (lowerRoom + upperRoom < deficit) return // Can't make enough room

        // Move points to make room
        const lowerMove = Math.min(lowerRoom, deficit)
        const upperMove = deficit - lowerMove
        if (lowerMove > 0) currentPoint[0] -= lowerMove
        if (upperMove > 0) nextPoint[0] += upperMove
    }

    const newTemp = (currentPoint[0] + nextPoint[0]) / 2
    // Ensure new point's duty is between previous and next point's duty
    const newDuty = Math.min(
        Math.max((currentPoint[1] + nextPoint[1]) / 2, currentPoint[1]),
        nextPoint[1],
    )

    data.splice(afterIdx + 1, 0, {
        value: [newTemp, newDuty],
        symbolSize: selectedSymbolSize,
        itemStyle: { color: selectedSymbolColor },
    })

    createGraphicDataFromPointData()
    // @ts-ignore
    option.series[0].data = data
    // @ts-ignore
    option.graphic = graphicData
    controlGraph.value?.setOption(option)

    selectedPointIndex.value = afterIdx + 1
    setTempAndDutyValues(afterIdx + 1)
    tableDataKey.value++
    contextIsDirty.value = true
}

// Remove point at index
const removePointFromTable = (idx: number): void => {
    if (data.length <= selectedTempSource!.profileMinLength) return
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
    contextIsDirty.value = true
}

// Check if point can be removed
const canRemovePoint = (idx: number): boolean => {
    return (
        data.length > selectedTempSource!.profileMinLength && idx !== 0 && idx !== data.length - 1
    )
}

// Check if point can be added after this index (considers moving adjacent points to make room)
const canAddPointAfter = (idx: number): boolean => {
    if (data.length >= selectedTempSource!.profileMaxLength) return false
    if (idx >= data.length - 1) return false

    const currentPoint = data[idx].value
    const nextPoint = data[idx + 1].value
    const tempGap = nextPoint[0] - currentPoint[0]
    const requiredGap = MIN_TEMP_SEPARATION * 2

    if (tempGap >= requiredGap) return true

    // Check if we can make room by moving adjacent points
    const deficit = requiredGap - tempGap

    let lowerMinTemp: number
    if (idx === 0) {
        lowerMinTemp = Math.max(selectedTempSource!.tempMin, axisXTempMin.value)
    } else {
        lowerMinTemp = data[idx - 1].value[0] + MIN_TEMP_SEPARATION
    }
    const lowerRoom = Math.max(0, currentPoint[0] - lowerMinTemp)

    let upperMaxTemp: number
    if (idx + 1 === data.length - 1) {
        upperMaxTemp = Math.min(selectedTempSource!.tempMax, axisXTempMax.value)
    } else {
        upperMaxTemp = data[idx + 2].value[0] - MIN_TEMP_SEPARATION
    }
    const upperRoom = Math.max(0, upperMaxTemp - nextPoint[0])

    return lowerRoom + upperRoom >= deficit
}

const showGraph = computed(() => {
    const shouldShow =
        selectedType.value != null &&
        selectedType.value === ProfileType.Graph &&
        chosenTemp.value != null
    if (shouldShow) {
        setTimeout(() => {
            // debounce because we need to wait for the graph to be rendered
            const resizeObserver = new ResizeObserver(
                _.debounce(
                    () => {
                        controlGraph.value?.setOption({
                            graphic: data.map(function (item, _dataIndex) {
                                return {
                                    type: 'circle',
                                    position: controlGraph.value?.convertToPixel(
                                        'grid',
                                        item.value,
                                    ),
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
const showStaticOffsetKnob = computed(() => {
    const shouldShow =
        selectedType.value != null &&
        selectedType.value === ProfileType.Overlay &&
        chosenOverlayOffsetType.value === 'static'
    if (shouldShow) {
        if (
            currentProfile.value.offset_profile != null &&
            currentProfile.value.offset_profile.length === 1
        ) {
            selectedStaticOffset.value = currentProfile.value.offset_profile[0][1]
        } else {
            selectedStaticOffset.value = 0
        }
    }
    return shouldShow
})

const showMixChart = computed(
    () =>
        selectedType.value != null &&
        selectedType.value === ProfileType.Mix &&
        chosenMemberProfiles.value.length > 1,
)
const showOverlayChart = computed(
    () =>
        selectedType.value != null &&
        selectedType.value === ProfileType.Overlay &&
        chosenOverlayMemberProfile.value != null,
)
const mixProfileKeys: Ref<string> = computed(() =>
    chosenMemberProfiles.value.map((p) => p.uid).join(':'),
)

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
        // return selectedTempSource.tempMin // starting point is horizontally fixed
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

// const editFunctionEnabled = () => {
//     return currentProfile.value.uid !== '0' && chosenFunction.value.uid !== '0'
// }
// const goToFunction = (): void => {
//     // router.push({ name: 'function', params: { functionUID: chosenFunction.value.uid } })
//     // dialogRef.value.close({ functionUID: chosenFunction.value.uid })
// }

const saveProfileState = async () => {
    if (currentProfile.value.uid === '0') {
        console.error('Changing of the default Profile is not allowed.')
        return
    }
    currentProfile.value.p_type = selectedType.value
    if (currentProfile.value.p_type === ProfileType.Fixed) {
        currentProfile.value.speed_fixed = selectedDuty.value
        currentProfile.value.speed_profile.length = 0
        currentProfile.value.temp_source = undefined
        currentProfile.value.temp_min = undefined
        currentProfile.value.temp_max = undefined
        currentProfile.value.function_uid = '0' // default function
        currentProfile.value.member_profile_uids.length = 0
        currentProfile.value.mix_function_type = undefined
        currentProfile.value.offset_profile = []
    } else if (currentProfile.value.p_type === ProfileType.Graph) {
        if (selectedTempSource === undefined) {
            tempSourceInvalid.value = true
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: t('views.profiles.tempSourceRequired'),
                life: 4000,
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
        currentProfile.value.temp_min = axisXTempMin.value
        currentProfile.value.temp_max = axisXTempMax.value
        currentProfile.value.function_uid = chosenFunction.value.uid
        currentProfile.value.speed_fixed = undefined
        currentProfile.value.member_profile_uids.length = 0
        currentProfile.value.mix_function_type = undefined
        currentProfile.value.offset_profile = []
    } else if (currentProfile.value.p_type === ProfileType.Mix) {
        if (chosenMemberProfiles.value.length < 2) {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: t('views.profiles.memberProfilesRequired'),
                life: 4000,
            })
            return
        }
        currentProfile.value.speed_fixed = undefined
        currentProfile.value.speed_profile.length = 0
        currentProfile.value.temp_source = undefined
        currentProfile.value.temp_min = undefined
        currentProfile.value.temp_max = undefined
        currentProfile.value.function_uid = '0' // default function
        currentProfile.value.member_profile_uids = chosenMemberProfiles.value.map((p) => p.uid)
        currentProfile.value.mix_function_type = chosenProfileMixFunction.value
        currentProfile.value.offset_profile = []
    } else if (currentProfile.value.p_type === ProfileType.Overlay) {
        if (chosenOverlayMemberProfile.value == null) {
            console.error('Overlay member profile can not be empty')
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: t('views.profiles.baseProfileRequired'),
                life: 4000,
            })
            return
        }
        currentProfile.value.speed_fixed = undefined
        currentProfile.value.speed_profile.length = 0
        currentProfile.value.temp_source = undefined
        currentProfile.value.temp_min = undefined
        currentProfile.value.temp_max = undefined
        currentProfile.value.function_uid = '0' // default function
        currentProfile.value.member_profile_uids = [chosenOverlayMemberProfile.value.uid]
        currentProfile.value.mix_function_type = undefined
        const offsetProfile: Array<[number, number]> = []
        if (chosenOverlayOffsetType.value === 'static') {
            // static offset uses a single profile point
            offsetProfile.push([50, selectedStaticOffset.value ?? 0])
        } else {
            for (const point of selectedGraphOffset.value) {
                offsetProfile.push(point)
            }
        }
        currentProfile.value.offset_profile = offsetProfile
    }
    const successful = await settingsStore.updateProfile(currentProfile.value.uid)
    if (successful) {
        contextIsDirty.value = false
        toast.add({
            severity: 'success',
            summary: t('common.success'),
            detail: t('views.profiles.profileUpdated'),
            life: 3000,
        })
    } else {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('views.profiles.profileUpdateError'),
            life: 3000,
        })
    }
}
const saveNameFunction = async (newName: string): Promise<boolean> => {
    if (newName.length > 0) {
        const oldName = currentProfile.value.name
        currentProfile.value.name = newName
        const successful = await settingsStore.updateProfile(currentProfile.value.uid)
        if (successful) {
            emitter.emit('profile-name-update', {
                profileUID: currentProfile.value.uid,
                name: newName,
            })
            return true
        } else {
            currentProfile.value.name = oldName
            return false
        }
    }
    return false
}

const tempScrolled = (event: WheelEvent): void => {
    if (selectedTemp.value == null) return
    if (event.deltaY < 0) {
        if (selectedTemp.value < inputNumberTempMax.value) selectedTemp.value += 1
    } else {
        if (selectedTemp.value > inputNumberTempMin.value) selectedTemp.value -= 1
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
    if (!contextIsDirty.value && contextIsVerifiedClean()) {
        next()
        return
    }
    confirm.require({
        message: t('views.profiles.unsavedChanges'),
        header: t('views.profiles.unsavedChangesHeader'),
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        rejectLabel: t('common.stay'),
        acceptLabel: t('common.discard'),
        accept: () => {
            next()
            contextIsDirty.value = false
        },
        reject: () => next(false),
    })
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
const updateKnobSize = (): void => {
    const el = document.getElementById('profile-display')
    if (el == null) {
        return
    }
    const w = el.getBoundingClientRect().width
    const h = el.getBoundingClientRect().height
    knobSize.value = Math.max(
        Math.min(w, h) - deviceStore.getREMSize(4),
        deviceStore.getREMSize(27),
    )
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
    setTimeout(updateKnobSize)
    window.addEventListener('resize', updateKnobSize)
    addScrollEventListeners()
    // re-add some scroll event listeners for elements that are rendered on Type change
    watch(selectedType, () => {
        nextTick(addScrollEventListeners)
    })
    watch([selectedType, chosenTemp], () => {
        setTimeout(updateResponsiveGraphHeight)
    })
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

    watch(
        [
            chosenMemberProfiles,
            chosenTemp,
            chosenFunction,
            chosenProfileMixFunction,
            selectedType,
            axisXTempMin,
            axisXTempMax,
            chosenOverlayOffsetType,
            chosenOverlayMemberProfile,
            selectedStaticOffset,
            selectedGraphOffset,
        ],
        () => {
            contextIsDirty.value = true
        },
    )
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)
})
onUnmounted(() => {
    window.removeEventListener('resize', updateResponsiveGraphHeight)
    window.removeEventListener('resize', updatePosition)
    window.removeEventListener('resize', updateKnobSize)
    stopRepeat()
})

// Prevent the Knob component from consuming non-left mouse button clicks (e.g. browser
// back/forward buttons). Stop the event before it reaches PrimeVue's handler, then
// manually trigger history navigation so the browser gesture still works.
function onKnobMousedown(e: MouseEvent) {
    if (e.button === 0) return
    e.stopImmediatePropagation()
    e.preventDefault()
}
function onKnobMouseup(e: MouseEvent) {
    if (e.button !== 3 && e.button !== 4) return
    if (e.button === 3) window.history.back()
    else if (e.button === 4) window.history.forward()
}
</script>

<template>
    <div id="control-panel" class="flex border-b-4 border-border-one items-center justify-between">
        <entity-title-rename
            :current-name="currentProfile.name"
            :save-name-function="saveNameFunction"
        />
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div v-if="selectedType === ProfileType.Mix" class="p-2 pr-0 flex flex-row">
                <Select
                    v-model="chosenProfileMixFunction"
                    :options="mixFunctionTypeOptions"
                    option-label="label"
                    option-value="value"
                    :placeholder="t('views.profiles.mixFunction')"
                    class="max-w-48 mr-3"
                    checkmark
                    dropdown-icon="pi pi-sliders-v"
                    scroll-height="40rem"
                    v-tooltip.bottom="t('views.profiles.applyMixFunction')"
                />
                <MultiSelect
                    v-model="chosenMemberProfiles"
                    :options="memberProfileOptions"
                    option-label="name"
                    :placeholder="t('views.profiles.memberProfiles')"
                    class="max-w-48"
                    scroll-height="40rem"
                    dropdown-icon="pi pi-chart-line"
                    v-tooltip.bottom="t('views.profiles.profilesToMix')"
                    :invalid="chosenMemberProfiles.length < 2"
                />
            </div>
            <div v-else-if="selectedType === ProfileType.Overlay" class="p-2 pr-0 flex flex-row">
                <InputNumber
                    v-if="chosenOverlayOffsetType === 'static'"
                    :placeholder="t('common.offset')"
                    v-model="selectedStaticOffset"
                    mode="decimal"
                    class="w-full h-[2.375rem] mr-3"
                    :suffix="` ${t('common.percentUnit')}`"
                    :prefix="staticOffsetPrefix"
                    showButtons
                    :min="offsetMin"
                    :max="offsetMax"
                    :use-grouping="false"
                    :step="1"
                    button-layout="horizontal"
                    :input-style="{ width: '5rem' }"
                    :disabled="chosenOverlayMemberProfile == null"
                    v-tooltip.bottom="t('views.profiles.staticOffset')"
                >
                    <template #incrementicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
                <Select
                    v-model="chosenOverlayOffsetType"
                    :options="overlayOffsetTypeOptions"
                    option-label="label"
                    option-value="value"
                    :placeholder="t('views.profiles.offsetType')"
                    class="max-w-48 mr-3"
                    checkmark
                    dropdown-icon="pi pi-sliders-v"
                    scroll-height="40rem"
                    v-tooltip.bottom="t('views.profiles.offsetType')"
                />
                <Select
                    v-model="chosenOverlayMemberProfile"
                    :options="offsetMemberProfileOptions"
                    option-label="name"
                    :placeholder="t('views.profiles.baseProfile')"
                    class="max-w-48"
                    scroll-height="40rem"
                    dropdown-icon="pi pi-chart-line"
                    :invalid="chosenOverlayMemberProfile == null"
                    v-tooltip.bottom="t('views.profiles.baseProfile')"
                />
            </div>
            <div v-else-if="selectedType === ProfileType.Graph" class="flex flex-wrap justify-end">
                <div class="p-2 pr-1">
                    <Select
                        v-model="chosenFunction"
                        :options="settingsStore.functions"
                        option-label="name"
                        :placeholder="t('views.profiles.function')"
                        class="w-44 h-[2.375rem]"
                        checkmark
                        dropdown-icon="pi pi-directions"
                        scroll-height="40rem"
                        v-tooltip.bottom="t('views.profiles.functionToApply')"
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
                        :placeholder="t('views.profiles.tempSource')"
                        :filter-placeholder="t('common.search')"
                        filter
                        checkmark
                        scroll-height="40rem"
                        :invalid="chosenTemp == null || tempSourceInvalid"
                        dropdown-icon="pi pi-inbox"
                        v-tooltip.bottom="{ escape: false, value: t('views.profiles.tempSource') }"
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
                                <div>{{ slotProps.option.temp }} {{ t('common.tempUnit') }}</div>
                            </div>
                        </template>
                    </Select>
                </div>
            </div>
            <div v-else-if="selectedType === ProfileType.Fixed" class="p-2 pr-0">
                <InputNumber
                    :placeholder="t('common.duty')"
                    v-model="selectedDuty"
                    inputId="selected-duty"
                    mode="decimal"
                    class="duty-input w-full h-[2.375rem]"
                    :suffix="` ${t('common.percentUnit')}`"
                    showButtons
                    :min="dutyMin"
                    :max="dutyMax"
                    :disabled="selectedPointIndex == null && !showDutyKnob"
                    :use-grouping="false"
                    :step="1"
                    button-layout="horizontal"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.bottom="t('views.profiles.fixedDuty')"
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
                    :options="profileTypeOptions"
                    option-label="label"
                    option-value="value"
                    :placeholder="t('views.profiles.profileType')"
                    class="w-[6.5rem] h-[2.375rem] mr-3"
                    dropdown-icon="pi pi-chart-line"
                    scroll-height="400px"
                    checkmark
                    v-tooltip.bottom="{
                        escape: false,
                        value: t('views.profiles.tooltip.profileType'),
                    }"
                />
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    :class="{ 'animate-pulse-fast': contextIsDirty }"
                    :label="t('common.save')"
                    v-tooltip.bottom="t('views.profiles.saveProfile')"
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
    <div v-if="showGraph" class="flex">
        <div class="flex flex-row justify-between mt-4 w-full">
            <InputNumber
                :placeholder="t('components.axisOptions.min')"
                v-model="axisXTempMin"
                mode="decimal"
                class="h-11 mx-4"
                :suffix="` ${t('common.tempUnit')}`"
                showButtons
                :min="inputAxisMinNumberMin"
                :max="inputAxisMinNumberMax"
                :use-grouping="false"
                :step="5"
                button-layout="horizontal"
                :input-style="{ width: '5rem' }"
                :disabled="selectedTempSource == null"
                v-tooltip.right="t('views.profiles.minProfileTemp')"
            >
                <template #incrementicon>
                    <span class="pi pi-plus" />
                </template>
                <template #decrementicon>
                    <span class="pi pi-minus" />
                </template>
            </InputNumber>
            <div class="flex flex-row">
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
            <InputNumber
                :placeholder="t('components.axisOptions.max')"
                v-model="axisXTempMax"
                mode="decimal"
                class="h-11 mx-4"
                :suffix="` ${t('common.tempUnit')}`"
                showButtons
                :min="inputAxisMaxNumberMin"
                :max="inputAxisMaxNumberMax"
                :use-grouping="false"
                :step="5"
                button-layout="horizontal"
                :input-style="{ width: '5rem' }"
                :disabled="selectedTempSource == null"
                v-tooltip.left="t('views.profiles.maxProfileTemp')"
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
        <div v-if="selectedType === ProfileType.Default" class="text-center text-3xl m-8">
            {{ t('views.profiles.systemDefault') }}
        </div>
        <div
            v-else-if="showDutyKnob"
            @mousedown.capture="onKnobMousedown"
            @mouseup.capture="onKnobMouseup"
        >
            <Knob
                v-model="selectedDuty"
                class="duty-knob-input m-2 w-full h-full flex justify-center"
                :value-template="(value) => `${value}${t('common.percentUnit')}`"
                :min="dutyMin"
                :max="dutyMax"
                :step="1"
                :stroke-width="deviceStore.getREMSize(0.75)"
                :size="knobSize"
            />
        </div>
        <div v-else-if="showGraph" class="relative">
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
                            <th class="px-1 py-1 text-center">{{ t('common.temperature') }}</th>
                            <th class="px-1 py-1 text-center">{{ t('common.duty') }}</th>
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
                                'hover:bg-bg-one/20':
                                    idx !== selectedPointIndex && idx !== data.length - 1,
                            }"
                        >
                            <!-- Point Index -->
                            <td
                                class="px-2 py-0.5 text-text-color-secondary cursor-pointer"
                                @click="selectPointFromTable(idx)"
                            >
                                {{ idx + 1 }}
                            </td>

                            <!-- Temperature Cell with +/- buttons -->
                            <td class="pr-2 py-1">
                                <div
                                    class="flex items-center justify-center gap-0.5"
                                    @wheel.prevent="
                                        idx !== 0 &&
                                        idx !== data.length - 1 &&
                                        handleTempScroll($event, idx)
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
                                            data[idx].value[0] <= getPointTempMin(idx)
                                        "
                                        @pointerdown.stop="
                                            startRepeat(() => decrementPointTemp(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    />
                                    <InputNumber
                                        :modelValue="point.value[0]"
                                        @update:modelValue="handleTempInput(idx, $event)"
                                        @focus="selectPointFromTable(idx)"
                                        mode="decimal"
                                        :minFractionDigits="1"
                                        :maxFractionDigits="1"
                                        :min="getPointTempMin(idx)"
                                        :max="getPointTempMax(idx)"
                                        :suffix="t('common.tempUnit')"
                                        :disabled="idx === 0 || idx === data.length - 1"
                                        :inputStyle="{
                                            width: '3.75rem',
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
                                            data[idx].value[0] >= getPointTempMax(idx)
                                        "
                                        @pointerdown.stop="
                                            startRepeat(() => incrementPointTemp(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    />
                                </div>
                            </td>

                            <!-- Duty Cell with +/- buttons -->
                            <td class="px-2 py-0.5">
                                <div
                                    class="flex items-center justify-center gap-0.5"
                                    @wheel.prevent="
                                        idx !== data.length - 1 && handleDutyScroll($event, idx)
                                    "
                                >
                                    <Button
                                        icon="pi pi-minus"
                                        text
                                        size="small"
                                        class="!w-5 !h-5 !p-0 [&>span]:text-[0.6rem]"
                                        :disabled="
                                            idx === data.length - 1 || data[idx].value[1] <= dutyMin
                                        "
                                        @pointerdown.stop="
                                            startRepeat(() => decrementPointDuty(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    />
                                    <InputNumber
                                        :modelValue="point.value[1]"
                                        @update:modelValue="handleDutyInput(idx, $event)"
                                        @focus="selectPointFromTable(idx)"
                                        mode="decimal"
                                        :minFractionDigits="0"
                                        :maxFractionDigits="0"
                                        :min="dutyMin"
                                        :max="dutyMax"
                                        :suffix="t('common.percentUnit')"
                                        :disabled="idx === data.length - 1"
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
                                            idx === data.length - 1 || data[idx].value[1] >= dutyMax
                                        "
                                        @pointerdown.stop="
                                            startRepeat(() => incrementPointDuty(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    />
                                </div>
                            </td>

                            <!-- Add/Remove Actions -->
                            <td class="px-0.5 py-0.5">
                                <div class="flex gap-0.5 opacity-0 group-hover:opacity-100">
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
        <MixProfileEditorChart
            v-else-if="showMixChart"
            class="p-6"
            :profiles="chosenMemberProfiles"
            :mixFunctionType="chosenProfileMixFunction"
            :key="mixProfileKeys"
        />
        <div
            v-else-if="showStaticOffsetKnob"
            @mousedown.capture="onKnobMousedown"
            @mouseup.capture="onKnobMouseup"
        >
            <Knob
                v-model="selectedStaticOffset"
                class="m-2 w-full h-full flex justify-center outline-none !outline-0"
                :value-template="
                    (value) =>
                        value > 0
                            ? `+${value}${t('common.percentUnit')}`
                            : `${value}${t('common.percentUnit')}`
                "
                :min="offsetMin"
                :max="offsetMax"
                :step="1"
                :stroke-width="deviceStore.getREMSize(0.75)"
                :size="knobSize"
                :disabled="chosenOverlayMemberProfile == null"
            />
        </div>
        <OverlayProfileEditorChart
            v-else-if="showOverlayChart"
            :profile-u-i-d="currentProfile.uid"
            @changed="(points) => (selectedGraphOffset = points)"
        />
    </div>
</template>

<style scoped lang="scss">
#control-graph {
    overflow: hidden;
    // This is adjusted dynamically on resize with js above
    height: max(calc(100vh - 8rem), 20rem);
    //width: max(calc(90vw - 17rem), 30rem);
    cursor: default;
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

// This is needed particularly in Tauri, as it moves to multiline flex-wrap as soon as the scrollbar
//  appears. Other browsers don't do this, so we need to force it to nowrap.
//.grid-webkit-fix {
//    @media screen and (min-width: 38rem) {
//        -webkit-flex-wrap: nowrap;
//    }
//}
</style>
