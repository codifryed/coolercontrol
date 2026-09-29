<!--
  SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiAlertOutline,
    mdiArrowTopRightBottomLeft,
    mdiChartMultiple,
    mdiContentDuplicate,
    mdiContentSaveOutline,
    mdiDeleteOutline,
    mdiExportVariant,
    mdiFan,
    mdiMinus,
    mdiPlus,
    mdiPlusCircleOutline,
} from '@mdi/js'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import type { OverlayPosition } from '@/models/UISettings.ts'
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
import { useToast } from '@/shell/toast'
import { $enum } from 'ts-enum-util'
import MixProfileEditorChart from '@/components/MixProfileEditorChart.vue'
import {
    onBeforeRouteLeave,
    onBeforeRouteUpdate,
    type RouteLocationRaw,
    useRouter,
} from 'vue-router'
import { useConfirm } from '@/shell/confirm'
import { useToolWizards } from '@/composables/useToolWizards.ts'
import _ from 'lodash'
import { useI18n } from 'vue-i18n'
import OverlayProfileEditorChart from '@/components/OverlayProfileEditorChart.vue'
import EntityTitleRename from '@/components/EntityTitleRename.vue'
import EntityPageHeader from '@/components/EntityPageHeader.vue'
import { Emitter, EventType } from 'mitt'
import { useProfileLimitInfo, type LimitInfo } from '@/composables/useProfileLimitInfo.ts'
import { defaultGraphCurve, placeholderGraphCurve } from '@/shell/cooling/defaultCurve.ts'
import HealthWarning from '@/components/HealthWarning.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import UiGroupedSelect from '@/shell/ui/UiGroupedSelect.vue'
import UiKnob from '@/shell/ui/UiKnob.vue'
import UiMultiSelect from '@/shell/ui/UiMultiSelect.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiSelect from '@/shell/ui/UiSelect.vue'
import { useLibraryGroups } from '@/shell/useLibraryGroups.ts'
import { type UiOptionGroup } from '@/shell/ui/UiGroupedListbox.vue'
import HelpIcon from '@/components/info/HelpIcon.vue'

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
    // Fixed graph height when embedded (e.g. '26rem'); viewport-based when absent.
    graphHeight?: string
    // Hides the internal save button; the host page saves via the exposed
    // saveProfileState (e.g. the channel page's Apply button).
    hideSave?: boolean
    // Channel context when embedded in a channel page; lets charts show the
    // channel's live Actual duty next to the calculated Target.
    channelDeviceUID?: string
    channelName?: string
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
const { getLimitInfo } = useProfileLimitInfo()

const contextIsDirty: Ref<boolean> = ref(false)
const tableDataKey: Ref<number> = ref(0)

// Deleting drops the profile from the store while this page is still mounted,
// and every render here dereferences it, so hold what was opened and fall back
// to it. The fallback covers the moment between the delete and the route change
// that unmounts this page, and the same moment when another client deletes it.
const openedProfile = settingsStore.profiles.find((profile) => profile.uid === props.profileUID)!
const currentProfile = computed(
    () =>
        settingsStore.profiles.find((profile) => profile.uid === props.profileUID) ?? openedProfile,
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
    limitInfo: LimitInfo | null
}

interface AvailableTempSources {
    deviceUID: string
    deviceName: string
    profileMinLength: number
    profileMaxLength: number
    amdGpuOverdrive?: boolean
    tempMin: number
    tempMax: number
    temps: Array<AvailableTemp>
}

interface CurrentTempSource {
    deviceUID: string
    deviceName: string
    profileMinLength: number
    profileMaxLength: number
    amdGpuOverdrive?: boolean
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
            amdGpuOverdrive: device.info.amd_gpu_overdrive,
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
                limitInfo: getLimitInfo({
                    profileMaxLength: deviceSource.profileMaxLength,
                    amdGpuOverdrive: deviceSource.amdGpuOverdrive,
                    tempName: temp.name,
                }),
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
            amdGpuOverdrive: tmpDevice.amdGpuOverdrive,
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
const selectedLimitInfo = computed<LimitInfo | null>(() => chosenTemp.value?.limitInfo ?? null)
const chosenFunction: Ref<Function> = ref(
    settingsStore.functions.find((f) => f.uid === currentProfile.value.function_uid)!,
)
const memberProfileOptions: Ref<Array<Profile>> = computed(() =>
    settingsStore.profiles.filter((profile) => {
        if (profile.uid === props.profileUID) return false
        if (profile.p_type === ProfileType.Graph) return true
        if (profile.p_type === ProfileType.Fixed) return true
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
    currentProfile.value.member_profile_uids.map((uid) =>
        settingsStore.profiles.find((profile) => profile.uid === uid)!,
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

// String-keyed models for the kit selects.
const selectedTypeModel = computed<string | undefined>({
    get: () => selectedType.value,
    set: (value) => {
        if (value != null) selectedType.value = value as ProfileType
    },
})
const chosenProfileMixFunctionModel = computed<string | undefined>({
    get: () => chosenProfileMixFunction.value,
    set: (value) => {
        if (value != null) chosenProfileMixFunction.value = value as ProfileMixFunctionType
    },
})
const chosenOverlayOffsetTypeModel = computed<string | undefined>({
    get: () => chosenOverlayOffsetType.value,
    set: (value) => {
        if (value != null) chosenOverlayOffsetType.value = value
    },
})
const { profileGroups, functionGroups: toFunctionGroups } = useLibraryGroups()
const memberProfileGroups = profileGroups(() =>
    memberProfileOptions.value.map((profile) => ({
        uid: profile.uid,
        name: profile.name,
    })),
)
const chosenMemberProfileUids = computed<string[]>({
    get: () => chosenMemberProfiles.value.map((profile) => profile.uid),
    set: (uids) => {
        chosenMemberProfiles.value = uids
            .map((uid) => settingsStore.profiles.find((profile) => profile.uid === uid))
            .filter((profile): profile is Profile => profile != null)
    },
})
const overlayBaseGroups = profileGroups(() =>
    offsetMemberProfileOptions.value.map((profile) => ({
        uid: profile.uid,
        name: profile.name,
    })),
)
const chosenOverlayMemberProfileUid = computed<string | undefined>({
    get: () => chosenOverlayMemberProfile.value?.uid,
    set: (uid) => {
        chosenOverlayMemberProfile.value = settingsStore.profiles.find(
            (profile) => profile.uid === uid,
        )
    },
})
const functionGroups = toFunctionGroups(() =>
    settingsStore.functions.map((fn) => ({ uid: fn.uid, name: fn.name })),
)
const chosenFunctionUid = computed<string | undefined>({
    get: () => chosenFunction.value.uid,
    set: (uid) => {
        const found = settingsStore.functions.find((fn) => fn.uid === uid)
        if (found != null) chosenFunction.value = found
    },
})
const tempKey = (deviceUID: string, tempName: string): string => `${deviceUID}/${tempName}`
const tempSourceGroups = computed<UiOptionGroup[]>(() =>
    tempSources.value.map((device) => ({
        label: device.deviceName,
        options: device.temps.map((temp) => ({
            label: temp.tempFrontendName,
            value: tempKey(temp.deviceUID, temp.tempName),
            color: temp.lineColor,
            rightText:
                temp.limitInfo != null
                    ? `${temp.limitInfo.badge} · ${temp.temp} ${t('common.tempUnit')}`
                    : `${temp.temp} ${t('common.tempUnit')}`,
        })),
    })),
)
const chosenTempKey = computed<string | undefined>({
    get: () =>
        chosenTemp.value != null
            ? tempKey(chosenTemp.value.deviceUID, chosenTemp.value.tempName)
            : undefined,
    set: (key) => {
        chosenTemp.value = tempSources.value
            .flatMap((device) => device.temps)
            .find((temp) => tempKey(temp.deviceUID, temp.tempName) === key)
    },
})
const selectedDutyModel = computed<number>({
    get: () => selectedDuty.value ?? 0,
    set: (value) => (selectedDuty.value = value),
})
const selectedStaticOffsetModel = computed<number>({
    get: () => selectedStaticOffset.value ?? 0,
    set: (value) => (selectedStaticOffset.value = value),
})
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

// Clamp the live temp indicator to the visible axis range so the line and its
// label stay pinned to the nearest edge instead of disappearing off-chart.
const clampTemp = (temp: number | undefined): number | undefined =>
    temp == null ? temp : Math.min(Math.max(temp, axisXTempMin.value), axisXTempMax.value)
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

const defaultDataValues = (): Array<PointData> => {
    const result: Array<PointData> = []
    if (selectedTempSource != null) {
        const curve = defaultGraphCurve(
            Math.max(selectedTempSource.tempMin, axisXTempMin.value),
            Math.min(selectedTempSource.tempMax, axisXTempMax.value),
            selectedTempSource.profileMinLength,
            selectedTempSource.profileMaxLength,
        )
        for (const [temp, duty] of curve) {
            result.push({
                value: [temp, duty],
                symbolSize: defaultSymbolSize,
                itemStyle: {
                    color: defaultSymbolColor,
                },
            })
        }
    } else {
        for (const [temp, duty] of placeholderGraphCurve()) {
            result.push({
                value: [temp, duty],
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

// ----- live Actual duty (channel context only) -----
// The Mix and Overlay editors already draw this line; the Graph editor is
// the same chart with the same channel context, so it draws it too. Only
// the actual is shown: the target is the curve itself.
const hasChannelContext = props.channelDeviceUID != null && props.channelName != null

const actualDutyLineData: [{ value: number[] }, { value: number[] }] = [
    { value: [] },
    { value: [] },
]

const actualDutyColor = (): string =>
    (hasChannelContext
        ? settingsStore.allUIDeviceSettings
              .get(props.channelDeviceUID!)
              ?.sensorsAndChannels.get(props.channelName!)?.color
        : undefined) || colors.themeColors.text_color

const getActualDuty = (): number | undefined => {
    if (!hasChannelContext) return undefined
    const duty = deviceStore.currentDeviceStatus
        .get(props.channelDeviceUID!)
        ?.get(props.channelName!)?.duty
    if (duty == null) return undefined
    const value = Number.parseFloat(duty)
    return Number.isNaN(value) ? undefined : value
}

const setActualDutyLine = (): number | undefined => {
    const duty = getActualDuty()
    if (duty == null) {
        actualDutyLineData[0].value = []
        actualDutyLineData[1].value = []
        return undefined
    }
    actualDutyLineData[0].value = [axisXTempMin.value, duty]
    actualDutyLineData[1].value = [axisXTempMax.value, duty]
    return duty
}

const actualDutyMarkData = (duty: number | undefined): Array<object> =>
    duty == null ? [] : [{ coord: [axisXTempMax.value - 5, duty], value: duty }]

const getDutyPosition = (duty: number): string => (duty < 91 ? 'top' : 'bottom')

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
        outerBoundsMode: 'same',
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
            zoomOnMouseWheel: 'ctrl',
            moveOnMouseWheel: false,
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
                        coord: [clampTemp(selectedTempSourceTemp.value), 95],
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

if (hasChannelContext) {
    const initialActualDuty = setActualDutyLine()
    option.series.push({
        id: 'actualDutyLine',
        type: 'line',
        smooth: false,
        symbol: 'none',
        lineStyle: {
            color: actualDutyColor(),
            width: 2,
            type: 'solid',
        },
        emphasis: {
            disabled: true,
        },
        data: actualDutyLineData,
        markPoint: {
            symbolSize: 0,
            label: {
                position: getDutyPosition(initialActualDuty ?? 0),
                align: 'right',
                fontSize: deviceStore.getREMSize(1.0),
                color: actualDutyColor(),
                formatter: (params: any): string => {
                    if (params.value == null) return ''
                    return (
                        t('views.profiles.actualDuty') + ' ' + Number(params.value).toFixed(0) + '%'
                    )
                },
                shadowColor: colors.themeColors.bg_one,
                shadowBlur: 10,
            },
            data: actualDutyMarkData(initialActualDuty),
        },
        z: 101,
        silent: true,
    } as any)
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
    option.series[2].markPoint.data[0].coord[0] = clampTemp(selectedTempSourceTemp.value)
    // @ts-ignore
    option.series[2].markPoint.data[0].value = selectedTempSourceTemp.value
    tempLineData[0].value = [clampTemp(selectedTempSourceTemp.value)!, dutyMin]
    tempLineData[1].value = [clampTemp(selectedTempSourceTemp.value)!, dutyMax]
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
    if (hasChannelContext) {
        const actualDuty = setActualDutyLine()
        controlGraph.value?.setOption({
            series: {
                id: 'actualDutyLine',
                data: actualDutyLineData,
                markPoint: {
                    data: actualDutyMarkData(actualDuty),
                    label: { position: getDutyPosition(actualDuty ?? 0) },
                },
            },
        })
    }
    if (selectedTempSource == null) {
        return
    }
    setTempSourceTemp()
    tempLineData[0].value = [clampTemp(selectedTempSourceTemp.value)!, dutyMin]
    tempLineData[1].value = [clampTemp(selectedTempSourceTemp.value)!, dutyMax]
    // there is a strange error only on the first time once switches back to a graph profile: Unknown series error
    controlGraph.value?.setOption({
        series: {
            id: 'tempLine',
            data: tempLineData,
            markPoint: {
                data: [
                    {
                        coord: [clampTemp(selectedTempSourceTemp.value), 95],
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
    graphicsCreated = true
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

// Points table position, persisted per profile: where the table sits out of the way depends on
// the shape of that profile's curve.
const tablePosition = computed({
    get: () => settingsStore.pointsTablePosition(props.profileUID),
    set: (position: OverlayPosition) =>
        settingsStore.setPointsTablePosition(props.profileUID, position),
})

const cycleTablePosition = () => {
    tablePosition.value = tablePosition.value === 'top-left' ? 'bottom-right' : 'top-left'
}

const tablePositionClasses = computed(() => ({
    'top-16 left-[5.5rem]': tablePosition.value === 'top-left',
    'bottom-16 right-[7rem]': tablePosition.value === 'bottom-right',
}))

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
    if (value == null || Number.isNaN(value) || idx === 0 || idx === data.length - 1) return
    const clampedTemp = Math.max(getPointTempMin(idx), Math.min(value, getPointTempMax(idx)))
    updatePointFromTable(idx, clampedTemp, data[idx].value[1])
}

const handleDutyInput = (idx: number, value: number | null): void => {
    if (value == null || Number.isNaN(value) || idx === data.length - 1) return
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

// Held at module scope so onUnmounted can reach them. Note this computed re-evaluates
// whenever selectedType or chosenTemp changes, so it re-enters the branch below repeatedly;
// releasing the previous observer and timeout first keeps exactly one of each alive.
let resizeObserver: ResizeObserver | null = null
let graphicsTimeout: ReturnType<typeof setTimeout> | null = null
let debouncedUpdatePosition: _.DebouncedFunc<() => void> | null = null
// The drag circles only exist once createDraggableGraphics has run for the
// current chart instance. A position-only update before that asks ECharts to
// create a graphic with no type, which throws and leaves the layer broken.
// The shell swaps its whole tree at 768px, so a narrow drag remounts this view
// and lands in that window on every resize event until the circles are back.
let graphicsCreated = false

const showGraph = computed(() => {
    const shouldShow =
        selectedType.value != null &&
        selectedType.value === ProfileType.Graph &&
        chosenTemp.value != null
    if (shouldShow) {
        if (graphicsTimeout !== null) clearTimeout(graphicsTimeout)
        resizeObserver?.disconnect()
        debouncedUpdatePosition?.cancel()
        graphicsCreated = false
        graphicsTimeout = setTimeout(() => {
            // debounce because we need to wait for the graph to be rendered
            debouncedUpdatePosition = _.debounce(updatePosition, 200, { leading: false })
            resizeObserver = new ResizeObserver(debouncedUpdatePosition)
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
// Both editable charts answer to the same mouse actions, so the header hint
// covers either one.
const showsEditableChart = computed(() => showGraph.value || showOverlayChart.value)
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
    // Deleting the Profile leaves this view mounted with nothing left to check.
    // Dereferencing it here would throw inside the route guard and block the redirect.
    if (currentProfile.value == null) return true
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
const checkForUnsavedChanges = (): boolean | Promise<boolean> => {
    if (!contextIsDirty.value && contextIsVerifiedClean()) {
        return true
    }
    return new Promise<boolean>((resolve) => {
        confirm.require({
            message: t('views.profiles.unsavedChanges'),
            header: t('views.profiles.unsavedChangesHeader'),
            icon: mdiAlertOutline,
            defaultFocus: 'accept',
            rejectLabel: t('common.stay'),
            acceptLabel: t('common.discard'),
            accept: () => {
                contextIsDirty.value = false
                resolve(true)
            },
            reject: () => resolve(false),
        })
    })
}
const router = useRouter()

const duplicateProfile = async (): Promise<void> => {
    const source = currentProfile.value
    // Copies, not the source's arrays: saveProfileState empties speed_profile and
    // member_profile_uids in place, so a shared array means editing the original
    // wipes the duplicate's curve.
    const newProfile = new Profile(
        `${source.name} ${t('common.copy')}`,
        source.p_type,
        source.speed_fixed,
        source.temp_source,
        source.speed_profile.map((point): [number, number] => [point[0], point[1]]),
        [...source.member_profile_uids],
        source.mix_function_type,
    )
    newProfile.function_uid = source.function_uid
    newProfile.temp_max = source.temp_max
    newProfile.temp_min = source.temp_min
    newProfile.offset_profile = source.offset_profile.map((point): [number, number] => [
        point[0],
        point[1],
    ])
    // saveProfile looks its subject up in the store, so the copy has to be pushed before
    // it can be persisted. Take it back out when the save fails: a phantom entry would
    // otherwise sit in the sidebar until a reload, and editing it would call
    // updateProfile against a UID the daemon has never seen.
    settingsStore.profiles.push(newProfile)
    if (!(await settingsStore.saveProfile(newProfile.uid))) {
        const index = settingsStore.profiles.findIndex((profile) => profile.uid === newProfile.uid)
        if (index >= 0) settingsStore.profiles.splice(index, 1)
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('views.profiles.profileUpdateError'),
            life: 3000,
        })
        return
    }
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('views.profiles.profileDuplicated'),
        life: 3000,
    })
    await router.push({ name: 'profiles', params: { profileUID: newProfile.uid } })
}

const deleteProfile = (): void => {
    if (currentProfile.value.uid === '0') return // can't delete default
    const associatedChannelSettings: Array<string> = []
    for (const [deviceUID, setting] of settingsStore.allDaemonDeviceSettings) {
        for (const channelSetting of setting.settings.values()) {
            if (channelSetting.profile_uid === currentProfile.value.uid) {
                associatedChannelSettings.push(
                    settingsStore.allUIDeviceSettings
                        .get(deviceUID)!
                        .sensorsAndChannels.get(channelSetting.channel_name)!.name,
                )
            }
        }
    }
    const deleteMessage: string =
        associatedChannelSettings.length === 0
            ? t('views.profiles.deleteProfileConfirm', { name: currentProfile.value.name })
            : t('views.profiles.deleteProfileWithChannelsConfirm', {
                  name: currentProfile.value.name,
                  channels: associatedChannelSettings.join(', '),
              })
    confirm.require({
        message: deleteMessage,
        header: t('views.profiles.deleteProfile'),
        icon: mdiAlertOutline,
        accept: async () => {
            contextIsDirty.value = false
            // Leave first: the store reloads several times inside the delete,
            // and every one of them re-renders a page whose profile is gone.
            const deletedUID = currentProfile.value.uid
            await router.push({ name: 'section-cooling' })
            await settingsStore.deleteProfile(deletedUID)
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('views.profiles.profileDeleted'),
                life: 3000,
            })
        },
    })
}

const { openProfileApplyWizard } = useToolWizards()

// Channels currently driven by this profile (where-used).
// Where-used: speed channels driven by this profile, plus Mix/Overlay profiles
// that reference it as a member or base (both stored in member_profile_uids).
interface UsedByItem {
    key: string
    label: string
    icon: string
    to: RouteLocationRaw
}
const usedByItems = computed((): UsedByItem[] => {
    const items: UsedByItem[] = []
    for (const [deviceUID, setting] of settingsStore.allDaemonDeviceSettings) {
        for (const channelSetting of setting.settings.values()) {
            if (channelSetting.profile_uid === currentProfile.value.uid) {
                const label =
                    settingsStore.allUIDeviceSettings
                        .get(deviceUID)
                        ?.sensorsAndChannels.get(channelSetting.channel_name)?.name ??
                    channelSetting.channel_name
                items.push({
                    key: `channel-${deviceUID}-${channelSetting.channel_name}`,
                    label,
                    icon: mdiFan,
                    to: {
                        name: 'cooling-channel',
                        params: { deviceUID, channelName: channelSetting.channel_name },
                    },
                })
            }
        }
    }
    for (const profile of settingsStore.profiles) {
        if (
            profile.uid !== currentProfile.value.uid &&
            profile.member_profile_uids.includes(currentProfile.value.uid)
        ) {
            items.push({
                key: `profile-${profile.uid}`,
                label: profile.name,
                icon: mdiChartMultiple,
                to: { name: 'profiles', params: { profileUID: profile.uid } },
            })
        }
    }
    return items
})

// Hidden until the first measurement sizes it, so the initial fit-to-window
// happens off-screen and fades in rather than showing as a resize flash.
const graphReady = ref(false)
const updateResponsiveGraphHeight = (): void => {
    const graphEl = document.getElementById('control-graph')
    const controlPanel = document.getElementById('control-panel')
    if (graphEl != null && props.graphHeight != null) {
        graphEl.style.height = props.graphHeight
        return
    }
    if (graphEl != null && controlPanel != null) {
        // Fill the viewport from wherever the graph starts (works inside the
        // shell content area). The small bottom inset mirrors the chart's side
        // padding so the axes sit roughly equidistant from the pane edges.
        const top = Math.ceil(graphEl.getBoundingClientRect().top)
        graphEl.style.height = `max(calc(100vh - ${top}px - 1rem), 20rem)`
    }
}
const updatePosition = (): void => {
    if (!graphicsCreated) {
        return
    }
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
    setTimeout(() => {
        updateResponsiveGraphHeight()
        graphReady.value = true
    })

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
    // Fixed Profiles have no graph points, so the duty is their only edit.
    // Graph Profiles also set selectedDuty on point selection, which is not an edit.
    watch(selectedDuty, (duty) => {
        if (selectedType.value !== ProfileType.Fixed) return
        if (duty !== currentProfile.value.speed_fixed) contextIsDirty.value = true
    })
    // An embedded editor saves through its host page (hideSave), which prompts
    // for its changes along with its own. Two guards would ask twice.
    if (!props.hideSave) {
        onBeforeRouteUpdate(checkForUnsavedChanges)
        onBeforeRouteLeave(checkForUnsavedChanges)
    }
})
onUnmounted(() => {
    window.removeEventListener('resize', updateResponsiveGraphHeight)
    window.removeEventListener('resize', updatePosition)
    window.removeEventListener('resize', updateKnobSize)
    stopRepeat()
    if (graphicsTimeout !== null) {
        clearTimeout(graphicsTimeout)
        graphicsTimeout = null
    }
    resizeObserver?.disconnect()
    resizeObserver = null
    debouncedUpdatePosition?.cancel()
    debouncedUpdatePosition = null
    graphicsCreated = false
})

// Prevent the knob from consuming non-left mouse button clicks (e.g. browser
// back/forward buttons). Stop the event before it reaches the knob's handler, then
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

// For host pages that embed this editor and drive saving themselves.
defineExpose({ saveProfileState, contextIsDirty })
</script>

<template>
    <entity-page-header id="control-panel">
        <template #title>
            <entity-title-rename
                :current-name="currentProfile.name"
                :save-name-function="saveNameFunction"
            />
            <HelpIcon
                v-if="showsEditableChart"
                class="ml-1"
                :label="t('common.mouseActions')"
                :text="t('views.profiles.graphProfileMouseActions')"
                side="bottom"
            />
            <HelpIcon
                v-else-if="showMixChart"
                class="ml-1"
                :text="t('views.profiles.targetHint')"
                side="bottom"
            />
        </template>
        <template #controls>
            <div class="p-2 pr-0">
                <span
                    v-tooltip.top="{
                        escape: false,
                        value: t('views.profiles.tooltip.profileType'),
                    }"
                >
                    <UiSelect
                        v-model="selectedTypeModel"
                        :options="profileTypeOptions"
                        :placeholder="t('views.profiles.profileType')"
                        class="w-44"
                    />
                </span>
            </div>
            <div v-if="selectedType === ProfileType.Mix" class="p-2 pr-0 flex flex-row">
                <span v-tooltip.top="t('views.profiles.applyMixFunction')" class="mr-3">
                    <UiSelect
                        v-model="chosenProfileMixFunctionModel"
                        :options="mixFunctionTypeOptions"
                        :placeholder="t('views.profiles.mixFunction')"
                        class="w-44"
                    />
                </span>
                <span v-tooltip.top="t('views.profiles.profilesToMix')">
                    <UiMultiSelect
                        v-model="chosenMemberProfileUids"
                        :groups="memberProfileGroups"
                        :placeholder="t('views.profiles.memberProfiles')"
                        class="w-44"
                        :invalid="chosenMemberProfiles.length < 2"
                    />
                </span>
            </div>
            <div v-else-if="selectedType === ProfileType.Overlay" class="p-2 pr-0 flex flex-row">
                <UiNumberInput
                    v-if="chosenOverlayOffsetType === 'static'"
                    v-model="selectedStaticOffsetModel"
                    class="mr-3"
                    :prefix="staticOffsetPrefix"
                    :suffix="t('common.percentUnit')"
                    :min="offsetMin"
                    :max="offsetMax"
                    :disabled="chosenOverlayMemberProfile == null"
                    v-tooltip.top="t('views.profiles.staticOffset')"
                />
                <span v-tooltip.top="t('views.profiles.offsetType')" class="mr-3">
                    <UiSelect
                        v-model="chosenOverlayOffsetTypeModel"
                        :options="overlayOffsetTypeOptions"
                        :placeholder="t('views.profiles.offsetType')"
                        class="w-44"
                    />
                </span>
                <span v-tooltip.top="t('views.profiles.baseProfile')">
                    <UiGroupedSelect
                        v-model="chosenOverlayMemberProfileUid"
                        :groups="overlayBaseGroups"
                        :placeholder="t('views.profiles.baseProfile')"
                        class="w-44"
                        :invalid="chosenOverlayMemberProfile == null"
                    />
                </span>
            </div>
            <div v-else-if="selectedType === ProfileType.Graph" class="flex flex-wrap justify-end">
                <div class="p-2 pr-1">
                    <span v-tooltip.top="{ escape: false, value: t('views.profiles.tempSource') }">
                        <UiGroupedSelect
                            v-model="chosenTempKey"
                            :groups="tempSourceGroups"
                            :placeholder="t('views.profiles.tempSource')"
                            filter
                            :filter-placeholder="t('common.search')"
                            class="w-44"
                            :invalid="chosenTemp == null || tempSourceInvalid"
                        />
                    </span>
                </div>
                <div class="p-2 pr-0">
                    <span v-tooltip.top="t('views.profiles.functionToApply')">
                        <UiGroupedSelect
                            v-model="chosenFunctionUid"
                            :groups="functionGroups"
                            :placeholder="t('views.profiles.function')"
                            class="w-44"
                        />
                    </span>
                </div>
            </div>
            <div v-else-if="selectedType === ProfileType.Fixed" class="p-2 pr-0">
                <UiNumberInput
                    v-model="selectedDutyModel"
                    class="duty-input"
                    :suffix="t('common.percentUnit')"
                    :min="dutyMin"
                    :max="dutyMax"
                    :disabled="selectedPointIndex == null && !showDutyKnob"
                    v-tooltip.top="t('views.profiles.fixedDuty')"
                />
            </div>
        </template>
        <template #actions>
            <template v-if="!hideSave">
                <UiButton
                    variant="ghost"
                    size="icon"
                    v-tooltip.top="t('components.wizards.profileApply.applyProfile')"
                    @click="openProfileApplyWizard(currentProfile.uid)"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiExportVariant"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                </UiButton>
                <UiButton
                    variant="ghost"
                    size="icon"
                    v-tooltip.top="t('layout.menu.tooltips.duplicate')"
                    @click="duplicateProfile"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiContentDuplicate"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                </UiButton>
                <UiButton
                    v-if="currentProfile.uid !== '0'"
                    variant="ghost"
                    size="icon"
                    v-tooltip.top="t('views.profiles.deleteProfile')"
                    @click="deleteProfile"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiDeleteOutline"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                </UiButton>
            </template>
            <div v-if="!hideSave" class="p-2">
                <UiButton
                    class="w-32"
                    :class="{ 'animate-pulse-fast': contextIsDirty }"
                    v-tooltip.top="t('views.profiles.saveProfile')"
                    @click="saveProfileState"
                >
                    <svg-icon
                        class="outline-0"
                        type="mdi"
                        :path="mdiContentSaveOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                </UiButton>
            </div>
        </template>
        <!-- Inside #control-panel so the chart-height observer accounts for it. -->
        <div
            v-if="!hideSave && usedByItems.length > 0"
            class="w-full mx-4 mb-2 flex flex-wrap items-center gap-x-1 gap-y-0.5 text-sm text-text-color-secondary"
        >
            <span>{{ t('views.profiles.usedBy') }}:</span>
            <span v-for="(item, index) in usedByItems" :key="item.key" class="whitespace-nowrap">
                <RouterLink
                    :to="item.to"
                    class="inline-flex items-center gap-1 rounded align-middle text-accent outline-none hover:underline focus-visible:ring-2 focus-visible:ring-accent"
                >
                    <svg-icon
                        type="mdi"
                        :path="item.icon"
                        :size="14"
                        class="shrink-0 text-text-color-secondary"
                    />
                    {{ item.label }}
                </RouterLink>
                <span v-if="index < usedByItems.length - 1">,</span>
            </span>
        </div>
        <health-warning kind="profile" :entity-uid="props.profileUID" class="w-full mx-2 mb-2" />
    </entity-page-header>
    <!-- The UI Display: -->
    <div v-if="showGraph" class="flex flex-col w-full">
        <div class="flex flex-row justify-between mt-4 w-full">
            <UiNumberInput
                v-model="axisXTempMin"
                class="mx-4"
                :suffix="t('common.tempUnit')"
                :min="inputAxisMinNumberMin"
                :max="inputAxisMinNumberMax"
                :step="5"
                :disabled="selectedTempSource == null"
                v-tooltip.top="t('views.profiles.minProfileTemp')"
            />
            <div class="flex flex-row items-center">
                <span v-if="selectedLimitInfo != null" class="text-sm opacity-70">
                    {{ selectedLimitInfo.message }}
                </span>
            </div>
            <UiNumberInput
                v-model="axisXTempMax"
                class="mx-4"
                :suffix="t('common.tempUnit')"
                :min="inputAxisMaxNumberMin"
                :max="inputAxisMaxNumberMax"
                :step="5"
                :disabled="selectedTempSource == null"
                v-tooltip.top="t('views.profiles.maxProfileTemp')"
            />
        </div>
    </div>
    <div id="profile-display" class="flex flex-col h-full">
        <div v-if="selectedType === ProfileType.Default" class="text-center text-3xl m-8">
            {{ t('common.unmanaged') }}
        </div>
        <div
            v-else-if="showDutyKnob"
            @mousedown.capture="onKnobMousedown"
            @mouseup.capture="onKnobMouseup"
        >
            <UiKnob
                v-model="selectedDutyModel"
                class="duty-knob-input m-2 w-full h-full flex justify-center"
                :value-template="(value: number) => `${value}${t('common.percentUnit')}`"
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
                class="pt-6 pr-11 pl-4 pb-4 transition-opacity duration-200"
                :class="{ 'opacity-0': !graphReady }"
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
                    <UiButton
                        variant="ghost"
                        size="icon"
                        class="!h-7 !w-7"
                        v-tooltip.top="t('views.profiles.moveTable')"
                        @click="cycleTablePosition"
                    >
                        <svg-icon type="mdi" :path="mdiArrowTopRightBottomLeft" :size="14" />
                    </UiButton>
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
                            class="group cursor-pointer"
                            :class="{
                                'bg-accent/30': idx === selectedPointIndex,
                                'hover:bg-bg-one/20':
                                    idx !== selectedPointIndex && idx !== data.length - 1,
                            }"
                            @click="selectPointFromTable(idx)"
                        >
                            <!-- Point Index -->
                            <td class="px-2 py-0.5 text-text-color-secondary">
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
                                    <UiButton
                                        variant="ghost"
                                        size="icon"
                                        class="!h-5 !w-5"
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
                                    >
                                        <svg-icon type="mdi" :path="mdiMinus" :size="10" />
                                    </UiButton>
                                    <input
                                        type="number"
                                        class="table-input h-6 w-[3.75rem] rounded bg-transparent px-0.5 text-center text-text-color outline-none focus:bg-bg-one disabled:opacity-60"
                                        :value="point.value[0].toFixed(1)"
                                        :min="getPointTempMin(idx)"
                                        :max="getPointTempMax(idx)"
                                        step="0.1"
                                        :disabled="idx === 0 || idx === data.length - 1"
                                        @change="
                                            handleTempInput(
                                                idx,
                                                ($event.target as HTMLInputElement).valueAsNumber,
                                            )
                                        "
                                        @focus="selectPointFromTable(idx)"
                                    />
                                    <UiButton
                                        variant="ghost"
                                        size="icon"
                                        class="!h-5 !w-5"
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
                                    >
                                        <svg-icon type="mdi" :path="mdiPlus" :size="10" />
                                    </UiButton>
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
                                    <UiButton
                                        variant="ghost"
                                        size="icon"
                                        class="!h-5 !w-5"
                                        :disabled="
                                            idx === data.length - 1 || data[idx].value[1] <= dutyMin
                                        "
                                        @pointerdown.stop="
                                            startRepeat(() => decrementPointDuty(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    >
                                        <svg-icon type="mdi" :path="mdiMinus" :size="10" />
                                    </UiButton>
                                    <input
                                        type="number"
                                        class="table-input h-6 w-[3rem] rounded bg-transparent px-0.5 text-center text-text-color outline-none focus:bg-bg-one disabled:opacity-60"
                                        :value="point.value[1].toFixed(0)"
                                        :min="dutyMin"
                                        :max="dutyMax"
                                        step="1"
                                        :disabled="idx === data.length - 1"
                                        @change="
                                            handleDutyInput(
                                                idx,
                                                ($event.target as HTMLInputElement).valueAsNumber,
                                            )
                                        "
                                        @focus="selectPointFromTable(idx)"
                                    />
                                    <UiButton
                                        variant="ghost"
                                        size="icon"
                                        class="!h-5 !w-5"
                                        :disabled="
                                            idx === data.length - 1 || data[idx].value[1] >= dutyMax
                                        "
                                        @pointerdown.stop="
                                            startRepeat(() => incrementPointDuty(idx))
                                        "
                                        @pointerup.stop="stopRepeat"
                                        @pointerleave="stopRepeat"
                                    >
                                        <svg-icon type="mdi" :path="mdiPlus" :size="10" />
                                    </UiButton>
                                </div>
                            </td>

                            <!-- Add/Remove Actions -->
                            <td class="px-0.5 py-0.5">
                                <div class="flex gap-0.5 opacity-0 group-hover:opacity-100">
                                    <UiButton
                                        v-if="canAddPointAfter(idx)"
                                        variant="ghost"
                                        size="icon"
                                        class="!h-6 !w-6 !text-success"
                                        v-tooltip.top="t('views.profiles.addPointAfter')"
                                        @click.stop="addPointFromTable(idx)"
                                    >
                                        <svg-icon
                                            type="mdi"
                                            :path="mdiPlusCircleOutline"
                                            :size="14"
                                        />
                                    </UiButton>
                                    <UiButton
                                        v-if="canRemovePoint(idx)"
                                        variant="ghost"
                                        size="icon"
                                        class="!h-6 !w-6 !text-error"
                                        v-tooltip.top="t('views.profiles.removePoint')"
                                        @click.stop="removePointFromTable(idx)"
                                    >
                                        <svg-icon type="mdi" :path="mdiDeleteOutline" :size="14" />
                                    </UiButton>
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
            :channel-device-u-i-d="channelDeviceUID"
            :channel-name="channelName"
            :key="mixProfileKeys"
        />
        <div
            v-else-if="showStaticOffsetKnob"
            @mousedown.capture="onKnobMousedown"
            @mouseup.capture="onKnobMouseup"
        >
            <UiKnob
                v-model="selectedStaticOffsetModel"
                class="m-2 w-full h-full flex justify-center"
                :value-template="
                    (value: number) =>
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
            :channel-device-u-i-d="props.channelDeviceUID"
            :channel-name="props.channelName"
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

// Points table inputs: no native number spinners.
.table-input::-webkit-outer-spin-button,
.table-input::-webkit-inner-spin-button {
    -webkit-appearance: none;
    margin: 0;
}
.table-input[type='number'] {
    -moz-appearance: textfield;
    appearance: textfield;
}

// This is needed particularly in Tauri, as it moves to multiline flex-wrap as soon as the scrollbar
//  appears. Other browsers don't do this, so we need to force it to nowrap.
//.grid-webkit-fix {
//    @media screen and (min-width: 38rem) {
//        -webkit-flex-wrap: nowrap;
//    }
//}
</style>
