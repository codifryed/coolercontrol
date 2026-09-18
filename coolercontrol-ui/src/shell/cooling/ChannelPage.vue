<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiAutoFix,
    mdiChartLine,
    mdiChevronDown,
    mdiShareVariantOutline,
    mdiSourceFork,
} from '@mdi/js'
import { computed, defineAsyncComponent, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useConfirm } from '@/shell/confirm'
import { useToast } from '@/shell/toast'
import { useCalibrationConversion } from '@/composables/useCalibrationConversion.ts'
import ChannelExtensionSettings from '@/components/ChannelExtensionSettings.vue'
import CalibrationBadge from '@/shell/cooling/CalibrationBadge.vue'
import ChannelVerdictNotice from '@/shell/cooling/ChannelVerdictNotice.vue'
import UncontrollableBadge from '@/shell/cooling/UncontrollableBadge.vue'
import FirmwareCurveBadge from '@/shell/cooling/FirmwareCurveBadge.vue'
import EntityTitleRename from '@/components/EntityTitleRename.vue'
import HealthWarning from '@/components/HealthWarning.vue'
import SpeedFixedChart from '@/components/SpeedFixedChart.vue'
import TimeChart from '@/components/TimeChart.vue'
import { v4 as uuidV4 } from 'uuid'
import { Dashboard, DashboardDeviceChannel } from '@/models/Dashboard.ts'
import type { UID } from '@/models/Device.ts'
import { DeviceType } from '@/models/Device.ts'
import type { CustomSensor } from '@/models/CustomSensor.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import ChainStrip, { type ChainPill } from '@/shell/cooling/ChainStrip.vue'
import { useChannelControl } from '@/shell/cooling/useChannelControl.ts'
import { useUnappliedGuard } from '@/shell/cooling/useUnappliedGuard.ts'
import ChannelSetupMenu from '@/shell/cooling/ChannelSetupMenu.vue'
import ControlFlowTree from '@/shell/cooling/ControlFlowTree.vue'
import { controlFlowExpanded as flowExpanded } from '@/shell/cooling/controlFlowState.ts'
import {
    buildFlowTree,
    flattenFlow,
    isFlowExpandable,
    type FlowNode,
} from '@/shell/cooling/controlFlow.ts'
import UiButton from '@/shell/ui/UiButton.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiGroupedSelect from '@/shell/ui/UiGroupedSelect.vue'
import { useLibraryGroups } from '@/shell/useLibraryGroups.ts'
import UiSlider from '@/shell/ui/UiSlider.vue'
import UiToggleGroup from '@/shell/ui/UiToggleGroup.vue'

// The original, fully featured profile editor, embedded with a fixed height.
const ProfileEditor = defineAsyncComponent(() => import('@/views/ProfileView.vue'))

const props = defineProps<{ deviceUID: UID; channelName: string }>()

const { t } = useI18n()
const toast = useToast()
const confirm = useConfirm()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const {
    speedOptions,
    controllable,
    uiSetting,
    channelLabel,
    defaultLabel,
    deviceLabel,
    saveChannelName,
    liveDuty,
    liveRpm,
    controlMode,
    manualDuty,
    selectedProfileUID,
    selectedProfile,
    sharedChannels,
    applying,
    setEditorRef,
    setExtensionSettingsRef,
    editorDirty,
    isDirty,
    canApply,
    apply,
} = useChannelControl(props.deviceUID, props.channelName)

useUnappliedGuard(isDirty)

const controlModeOptions = computed(() => [
    { label: t('layout.shell.coolingPage.modeProfile'), value: 'automatic' },
    { label: t('layout.shell.coolingPage.modeManual'), value: 'manual' },
    { label: t('layout.shell.coolingPage.modeUnmanaged'), value: 'unmanaged' },
])

const { profileGroups: toProfileGroups } = useLibraryGroups()
const profileGroups = toProfileGroups(() =>
    settingsStore.profiles.filter((profile) => profile.uid !== '0'),
)

// ----- chain strip -----
const chainPills = computed<ChainPill[]>(() => {
    if (controlMode.value === 'manual') {
        return [
            {
                kind: 'profile',
                label: t('layout.shell.coolingPage.manualAt', { duty: manualDuty.value }),
            },
        ]
    }
    if (controlMode.value === 'unmanaged' || selectedProfile.value == null) {
        return [{ kind: 'profile', label: t('common.unmanaged') }]
    }
    const pills: ChainPill[] = []
    const source = selectedProfile.value.temp_source
    if (source != null) {
        const custom = customSensor(source.device_uid, source.temp_name)
        pills.push({
            kind: 'tempSource',
            label: tempSourceLabel(source.device_uid, source.temp_name),
            color: tempSourceColor(source.device_uid, source.temp_name),
            to:
                custom != null
                    ? { name: 'device-custom-sensor', params: { customSensorID: source.temp_name } }
                    : {
                          name: 'monitoring-sensor',
                          params: {
                              deviceUID: source.device_uid,
                              channelName: source.temp_name,
                          },
                      },
        })
    }
    pills.push({
        kind: 'profile',
        label: selectedProfile.value.name,
        to:
            selectedProfile.value.uid !== '0'
                ? { name: 'profiles', params: { profileUID: selectedProfile.value.uid } }
                : undefined,
    })
    const fun = settingsStore.functions.find(
        (candidate) => candidate.uid === selectedProfile.value?.function_uid,
    )
    if (fun != null && fun.uid !== '0') {
        pills.push({
            kind: 'function',
            label: fun.name,
            to: { name: 'functions', params: { functionUID: fun.uid } },
        })
    }
    return pills
})

// Full influence tree behind the compact chain, revealed on demand for
// composite (Mix/Overlay) profiles whose members carry their own inputs.
const customSensorsByID = ref<Map<string, CustomSensor>>(new Map())
const customSensorsDeviceUID = computed<string | undefined>(() => {
    for (const device of deviceStore.allDevices()) {
        if (device.type === DeviceType.CUSTOM_SENSORS) return device.uid
    }
    return undefined
})
onMounted(async () => {
    const sensors = await settingsStore.getCustomSensors()
    const map = new Map<string, CustomSensor>()
    for (const sensor of sensors) map.set(sensor.id, sensor)
    customSensorsByID.value = map
})
const functionByUID = (uid: string): { uid: string; name: string } | undefined => {
    if (uid === '0') return undefined
    const fun = settingsStore.functions.find((candidate) => candidate.uid === uid)
    return fun != null ? { uid: fun.uid, name: fun.name } : undefined
}
const tempSourceLabel = (deviceUID: string, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.name ??
    channelName
const tempSourceColor = (deviceUID: string, channelName: string): string | undefined =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.color
const customSensor = (deviceUID: string, channelName: string): CustomSensor | undefined =>
    deviceUID === customSensorsDeviceUID.value
        ? customSensorsByID.value.get(channelName)
        : undefined
const flowTree = computed<FlowNode | null>(() => {
    if (
        controlMode.value === 'manual' ||
        controlMode.value === 'unmanaged' ||
        selectedProfile.value == null
    ) {
        return null
    }
    return buildFlowTree(selectedProfile.value, {
        profiles: settingsStore.profiles,
        functionByUID,
        sensorLabel: tempSourceLabel,
        customSensor,
    })
})
const flowRows = computed(() => (flowTree.value != null ? flattenFlow(flowTree.value) : []))
const flowExpandable = computed(() => flowTree.value != null && isFlowExpandable(flowTree.value))

// ----- fork -----
const confirmAction = async (
    header: string,
    message: string,
    acceptLabel: string,
): Promise<boolean> =>
    new Promise<boolean>((resolve) => {
        confirm.require({
            header,
            message,
            acceptLabel,
            rejectLabel: t('common.cancel'),
            defaultFocus: 'reject',
            accept: () => resolve(true),
            reject: () => resolve(false),
        })
    })

// Clones the shared profile so edits below only affect this fan.
const forkProfile = async (): Promise<void> => {
    const source = selectedProfile.value
    if (source == null || applying.value) return
    const confirmed = await confirmAction(
        t('layout.shell.coolingPage.fork.confirmHeader'),
        t('layout.shell.coolingPage.fork.confirmMessage', {
            profile: source.name,
            copy: conversion.forkName(source.name),
            channel: channelLabel.value,
        }),
        t('layout.shell.coolingPage.fork.accept'),
    )
    if (!confirmed) return
    applying.value = true
    try {
        const forked = await conversion.forkProfile(source)
        if (forked != null) selectedProfileUID.value = forked.uid
    } finally {
        applying.value = false
    }
}

// ----- calibration conversion -----
const conversion = useCalibrationConversion(
    props.deviceUID,
    props.channelName,
    () => channelLabel.value,
)

const canConvert = computed<boolean>(() => {
    if (!conversion.isConvertible.value || applying.value) return false
    if (controlMode.value === 'manual') return true
    if (controlMode.value !== 'automatic') return false
    return selectedProfile.value != null && conversion.canConvertProfile(selectedProfile.value)
})

const confirmConvert = async (message: string): Promise<boolean> =>
    confirmAction(
        t('layout.shell.coolingPage.convert.confirmHeader'),
        message,
        t('layout.shell.coolingPage.convert.accept'),
    )

/**
 * Converting is only correct once, so it always confirms first. The original
 * profile is never modified: a mistake is undone by deleting the fork.
 */
const convertForCalibration = async (): Promise<void> => {
    if (!canConvert.value) return
    if (controlMode.value === 'manual') {
        const confirmed = await confirmConvert(
            t('layout.shell.coolingPage.convert.confirmManual', { channel: channelLabel.value }),
        )
        if (!confirmed) return
        applying.value = true
        try {
            const converted = await conversion.convertManualDuty(manualDuty.value)
            if (converted == null) return
            manualDuty.value = converted
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('layout.shell.coolingPage.convert.successManual', { duty: converted }),
                life: 3000,
            })
        } finally {
            applying.value = false
        }
        return
    }
    const source = selectedProfile.value
    if (source == null) return
    const confirmed = await confirmConvert(
        t('layout.shell.coolingPage.convert.confirmProfile', {
            profile: source.name,
            copy: conversion.convertedName(source.name),
            channel: channelLabel.value,
        }),
    )
    if (!confirmed) return
    applying.value = true
    try {
        const forked = await conversion.convertProfile(source)
        if (forked == null) return
        selectedProfileUID.value = forked.uid
        toast.add({
            severity: 'success',
            summary: t('common.success'),
            detail: t('layout.shell.coolingPage.convert.successProfile', {
                profile: forked.name,
                channel: channelLabel.value,
            }),
            life: 3000,
        })
    } finally {
        applying.value = false
    }
}

// Chart canvases consume plain wheel events for zoom; stop them in the capture
// phase so the page scrolls. Ctrl+wheel (and trackpad pinch) passes through.
const onPageWheelCapture = (event: WheelEvent): void => {
    if (!event.ctrlKey) event.stopPropagation()
}

// ----- live chart -----
const createChannelDashboard = (): Dashboard => {
    const dash = new Dashboard(channelLabel.value)
    dash.timeRangeSeconds = 300
    dash.dataTypes = []
    dash.deviceChannelNames.push(new DashboardDeviceChannel(props.deviceUID, props.channelName))
    const setting = uiSetting.value
    if (setting != null) setting.channelDashboard = dash
    return dash
}
const channelDashboard = ref(uiSetting.value?.channelDashboard ?? createChannelDashboard())
if (channelDashboard.value.dataTypes.length > 0) {
    channelDashboard.value.dataTypes = []
}
// A chart binds its lines to its series at mount, so a changed line set needs a new chart.
const chartKey = ref<string>(uuidV4())
</script>

<template>
    <div class="flex h-full flex-col gap-4 overflow-y-auto p-4" @wheel.capture="onPageWheelCapture">
        <div class="flex flex-wrap items-start gap-3">
            <div class="min-w-0">
                <div class="flex items-baseline gap-2">
                    <EntityTitleRename
                        class="!py-0 !pl-0"
                        :current-name="channelLabel"
                        :fallback-name="defaultLabel"
                        :save-name-function="saveChannelName"
                    />
                    <span class="truncate text-base text-text-color-secondary">
                        {{ deviceLabel }}
                    </span>
                </div>
            </div>
            <div class="ml-auto flex items-center gap-3">
                <span class="text-2xl font-semibold font-numeric tabular-nums text-text-color">
                    {{ liveDuty != null ? `${liveDuty}%` : '--' }}
                </span>
                <span
                    v-if="liveRpm != null"
                    class="text-base font-numeric tabular-nums text-text-color-secondary"
                >
                    {{ liveRpm }} rpm
                </span>
            </div>
        </div>

        <HealthWarning kind="channel" :device-uid="deviceUID" :channel-name="channelName" />

        <ChainStrip
            :channel-label="channelLabel"
            :pills="chainPills"
            :expandable="flowExpandable"
            :expanded="flowExpanded"
            @toggle-expand="flowExpanded = !flowExpanded"
        />
        <ControlFlowTree v-if="flowExpanded && flowExpandable" :rows="flowRows" />

        <template v-if="controllable">
            <div class="flex flex-wrap items-center gap-3">
                <UiToggleGroup v-model="controlMode" :options="controlModeOptions" />
                <ChannelSetupMenu :device-u-i-d="deviceUID" :channel-name="channelName">
                    <template #trigger>
                        <UiButton variant="outline">
                            <svg-icon type="mdi" :path="mdiAutoFix" :size="16" />
                            {{ t('layout.shell.coolingPage.guidedSetup') }}
                            <svg-icon type="mdi" :path="mdiChevronDown" :size="16" />
                        </UiButton>
                    </template>
                </ChannelSetupMenu>
                <ChannelExtensionSettings
                    :ref="setExtensionSettingsRef"
                    :device-u-i-d="deviceUID"
                    :channel-name="channelName"
                    :chosen-profile="controlMode === 'automatic' ? selectedProfile : undefined"
                />
                <FirmwareCurveBadge
                    :device-u-i-d="deviceUID"
                    :channel-name="channelName"
                    :size="20"
                />
                <CalibrationBadge
                    :device-u-i-d="deviceUID"
                    :channel-name="channelName"
                    :size="20"
                />
                <UncontrollableBadge
                    :device-u-i-d="deviceUID"
                    :channel-name="channelName"
                    :size="20"
                />
                <UiButton
                    class="ml-auto"
                    :class="{ 'animate-pulse-fast': isDirty }"
                    :disabled="!canApply"
                    @click="apply"
                >
                    {{
                        editorDirty
                            ? t('layout.shell.coolingPage.saveAndApply')
                            : t('layout.shell.coolingPage.apply')
                    }}
                </UiButton>
            </div>

            <!-- Manual -->
            <div
                v-if="controlMode === 'manual'"
                class="flex flex-col gap-3 rounded-lg border border-border-one p-3"
            >
                <span class="text-sm text-text-color-secondary">
                    {{ t('layout.shell.coolingPage.manualDuty') }}
                </span>
                <div class="flex flex-wrap items-center gap-4">
                    <UiSlider
                        v-model="manualDuty"
                        :min="speedOptions?.min_duty ?? 0"
                        :max="speedOptions?.max_duty ?? 100"
                        class="w-full max-w-md"
                    />
                    <UiNumberInput
                        v-model="manualDuty"
                        :min="speedOptions?.min_duty ?? 0"
                        :max="speedOptions?.max_duty ?? 100"
                        :step="1"
                        suffix="%"
                    />
                    <UiButton
                        v-if="canConvert"
                        class="ml-auto"
                        variant="outline"
                        :disabled="applying"
                        v-tooltip.top="t('layout.shell.coolingPage.convert.tooltip')"
                        @click="convertForCalibration"
                    >
                        <svg-icon type="mdi" :path="mdiSourceFork" :size="14" />
                        {{ t('layout.shell.coolingPage.convert.button') }}
                    </UiButton>
                </div>
                <SpeedFixedChart
                    :duty="manualDuty"
                    :current-device-u-i-d="deviceUID"
                    :current-sensor-name="channelName"
                    style="--gauge-height: clamp(24rem, calc(100vh - 32rem), 44rem)"
                />
            </div>

            <!-- Unmanaged -->
            <div
                v-else-if="controlMode === 'unmanaged'"
                class="flex flex-col gap-3 rounded-lg border border-border-one p-3"
            >
                <p class="max-w-xl text-base text-text-color-secondary">
                    {{ t('layout.shell.coolingPage.unmanagedHint') }}
                </p>
                <SpeedFixedChart
                    :default-profile="true"
                    :current-device-u-i-d="deviceUID"
                    :current-sensor-name="channelName"
                    style="--gauge-height: clamp(24rem, calc(100vh - 32rem), 44rem)"
                />
            </div>

            <!-- Profile -->
            <div v-else class="flex flex-col gap-4">
                <div
                    class="flex flex-wrap items-end gap-x-4 gap-y-3 rounded-lg border border-border-one p-3"
                >
                    <div class="flex flex-col gap-1">
                        <span class="text-sm text-text-color-secondary">
                            {{ t('layout.shell.coolingPage.chain.profile') }}
                        </span>
                        <UiGroupedSelect
                            v-model="selectedProfileUID"
                            :groups="profileGroups"
                            :placeholder="t('layout.shell.coolingPage.selectProfile')"
                        />
                    </div>
                    <div v-if="selectedProfile != null" class="flex items-center gap-2">
                        <span
                            class="inline-flex h-10 items-center gap-1.5 rounded-lg border border-border-one bg-bg-two px-3 text-sm text-text-color-secondary"
                            v-tooltip.top="
                                sharedChannels.length > 0
                                    ? t('layout.shell.coolingPage.sharedTooltip')
                                    : t('layout.shell.coolingPage.notSharedTooltip')
                            "
                        >
                            <svg-icon type="mdi" :path="mdiShareVariantOutline" :size="13" />
                            {{
                                sharedChannels.length > 0
                                    ? t('layout.shell.coolingPage.sharedWith', {
                                          count: sharedChannels.length,
                                      })
                                    : t('layout.shell.coolingPage.notShared')
                            }}
                        </span>
                        <UiButton variant="outline" :disabled="applying" @click="forkProfile">
                            <svg-icon type="mdi" :path="mdiSourceFork" :size="14" />
                            {{ t('layout.shell.coolingPage.forkForFan') }}
                        </UiButton>
                    </div>
                    <UiButton
                        v-if="canConvert"
                        variant="outline"
                        :disabled="applying"
                        v-tooltip.top="t('layout.shell.coolingPage.convert.tooltip')"
                        @click="convertForCalibration"
                    >
                        <svg-icon type="mdi" :path="mdiSourceFork" :size="14" />
                        {{ t('layout.shell.coolingPage.convert.button') }}
                    </UiButton>
                </div>

                <div v-if="selectedProfileUID != null" class="rounded-lg border border-border-one">
                    <ProfileEditor
                        :ref="setEditorRef"
                        :key="selectedProfileUID"
                        :profile-u-i-d="selectedProfileUID"
                        :channel-device-u-i-d="deviceUID"
                        :channel-name="channelName"
                        graph-height="clamp(30rem, calc(100vh - 26rem), 44rem)"
                        hide-save
                    />
                </div>
            </div>
        </template>
        <ChannelVerdictNotice v-else :device-u-i-d="deviceUID" :channel-name="channelName" />

        <!-- relative lifts this above the ProfileEditor's empty overhang box,
             which otherwise swallows pointer events on the link and chart top. -->
        <div class="relative shrink-0" style="--time-chart-height: 24rem">
            <div class="mb-1 flex justify-center">
                <RouterLink
                    :to="{ name: 'monitoring-sensor', params: { deviceUID, channelName } }"
                    class="flex items-center gap-1 rounded-lg px-2 py-1 text-sm text-text-color-secondary outline-none hover:bg-surface-hover hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                >
                    <svg-icon type="mdi" :path="mdiChartLine" :size="16" />
                    {{ t('layout.shell.coolingPage.fullChart') }}
                </RouterLink>
            </div>
            <TimeChart
                :key="chartKey"
                :dashboard="channelDashboard"
                @line-set-changed="chartKey = uuidV4()"
            />
        </div>
    </div>
</template>
