<!--
  SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiAlertOutline,
    mdiBellSleepOutline,
    mdiContentDuplicate,
    mdiContentSaveOutline,
    mdiTrashCanOutline,
} from '@mdi/js'
import { computed, onMounted, ref, toRaw, type Ref, watch } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'reka-ui'
import AlertLogTable from '@/components/AlertLogTable.vue'
import { onBeforeRouteLeave, onBeforeRouteUpdate, useRoute, useRouter } from 'vue-router'
import { useConfirm } from '@/shell/confirm'
import UiButton from '@/shell/ui/UiButton.vue'
import UiGroupedListbox from '@/shell/ui/UiGroupedListbox.vue'
import UiLockToggle from '@/shell/ui/UiLockToggle.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiSlider from '@/shell/ui/UiSlider.vue'
import { Alert, alertIsSilenced, alertSources } from '@/models/Alert.ts'
import UiTag from '@/shell/ui/UiTag.vue'
import AlertSilenceMenu from '@/components/AlertSilenceMenu.vue'
import { ChannelMetric, ChannelSource } from '@/models/ChannelSource.ts'
import { useI18n } from 'vue-i18n'
import EntityTitleRename from '@/components/EntityTitleRename.vue'
import EntityPageHeader from '@/components/EntityPageHeader.vue'
import UiSettingRow from '@/shell/ui/UiSettingRow.vue'
import UiSettingsCard from '@/shell/ui/UiSettingsCard.vue'
import UiSwitch from '@/shell/ui/UiSwitch.vue'
import {
    alignUpToStep,
    hasThresholdLock,
    needsUnlock,
    THRESHOLD_GAP,
    thresholdMax,
    thresholdStep,
} from '@/components/alertThresholds.ts'
import { groupDigits } from '@/shell/digitGroups.ts'
import AlertSourceReference from '@/components/AlertSourceReference.vue'
import type { ThresholdTarget, Thresholds } from '@/components/alertReference.ts'

interface Props {
    alertUID?: string
}

const defaultMin: number = 0.0

interface AvailableChannel {
    deviceUID: string // needed here as well for the dropdown selector
    channelName: string
    channelFrontendName: string
    lineColor: string
    value: string
    metric: ChannelMetric
}

interface AvailableChannelSources {
    deviceUID: string
    deviceName: string
    channels: Array<AvailableChannel>
}

const props = defineProps<Props>()
const deviceStore = useDeviceStore()
// We need to use the raw state to watch for changes, as the pinia reactive proxy isn't properly
// reacting to changes from Vue's shallowRef & triggerRef anymore.
const rawStore = toRaw(deviceStore.$state)
const settingsStore = useSettingsStore()
const router = useRouter()
const route = useRoute()
const { t } = useI18n()
const confirm = useConfirm()

const contextIsDirty: Ref<boolean> = ref(false)
const shouldCreateAlert: boolean = !props.alertUID
const pollRate: Ref<number> = ref(settingsStore.ccSettings.poll_rate)

const collectAlert = async (): Promise<Alert> => {
    if (shouldCreateAlert) {
        const newAlertName = `${t('views.alerts.newAlert')} ${settingsStore.alerts.length + 1}`
        // Placeholder source; replaced from the selection on save.
        const channelSource = new ChannelSource('', '', ChannelMetric.Temp)
        return new Alert(newAlertName, [channelSource], defaultMin, 100, pollRate.value)
    } else {
        const foundAlert = settingsStore.alerts.find((alert) => alert.uid === props.alertUID!)
        if (foundAlert == undefined) {
            throw new Error(`Illegal State: Could not find Alert with UID: ${props.alertUID}`)
        }
        return foundAlert
    }
}
const alert: Alert = await collectAlert()
const chosenChannelKeys: Ref<Array<string>> = ref([])
const chosenMin: Ref<number> = ref(alert.min)
const chosenMax: Ref<number> = ref(alert.max)
const chosenName: Ref<string> = ref(alert.name)
const chosenWarmupDuration: Ref<number> = ref(alert.warmup_duration)
const chosenCooldownDuration: Ref<number> = ref(alert.cooldown_duration)
const chosenRepeatMinutes: Ref<number> = ref(Math.round(alert.repeat_interval / 60))
const chosenEnabled: Ref<boolean> = ref(alert.enabled)
const chosenDesktopNotification: Ref<boolean> = ref(alert.desktop_notify)
const chosenDesktopNotificationRecovery: Ref<boolean> = ref(alert.desktop_notify_recovery)
const chosenDesktopNotificationAudio: Ref<boolean> = ref(alert.desktop_notify_audio)
const chosenShutdownOnActivation: Ref<boolean> = ref(alert.shutdown_on_activation)

const channelSources: Ref<Array<AvailableChannelSources>> = ref([])
const fillChannelSources = async (): Promise<void> => {
    channelSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.status.temps.length === 0 && device.status.channels.length === 0) {
            continue
        }
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        const deviceSource: AvailableChannelSources = {
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            channels: [],
        }
        const addAvailableChannel = (
            channelName: string,
            value: number,
            metric: ChannelMetric,
        ): void => {
            deviceSource.channels.push({
                deviceUID: device.uid,
                channelName: channelName,
                channelFrontendName: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                lineColor: deviceSettings.sensorsAndChannels.get(channelName)!.color,
                value: value.toFixed(0),
                metric: metric,
            })
        }
        for (const temp of device.status.temps) {
            deviceSource.channels.push({
                deviceUID: device.uid,
                channelName: temp.name,
                channelFrontendName: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                lineColor: deviceSettings.sensorsAndChannels.get(temp.name)!.color,
                value: temp.temp.toFixed(1),
                metric: ChannelMetric.Temp,
            })
        }
        for (const channel of device.status.channels) {
            // Duties and RPMs are separate sources for Alerts:
            if (channel.duty != null) {
                const value = channel.duty
                const metric = channel.name.toLowerCase().includes('load')
                    ? ChannelMetric.Load
                    : ChannelMetric.Duty
                addAvailableChannel(channel.name, value, metric)
            }
            if (channel.rpm != null) {
                const value = channel.rpm
                const metric = ChannelMetric.RPM
                addAvailableChannel(channel.name, value, metric)
            }
        }
        if (deviceSource.channels.length === 0) {
            continue // when all of a devices channels are hidden
        }
        channelSources.value.push(deviceSource)
    }
}
await fillChannelSources()
// Metric-qualified: a fan channel appears once per metric (Duty and RPM).
const channelKey = (channel: AvailableChannel): string =>
    `${channel.deviceUID}/${channel.channelName}/${channel.metric}`
const findChannel = (
    deviceUID: string,
    channelName: string,
    metric: ChannelMetric,
): AvailableChannel | undefined =>
    channelSources.value
        .flatMap((device) => device.channels)
        .find(
            (channel) =>
                channel.deviceUID === deviceUID &&
                channel.channelName === channelName &&
                channel.metric === metric,
        )
const startingChannelKeys = (): Array<string> => {
    // Edit: use the alert's own sources. Create: honor an optional
    // ?device/&channel/&metric query (from the Monitoring "create alert" row
    // convenience) to preselect the source.
    if (!shouldCreateAlert) {
        return alertSources(alert)
            .map((source) =>
                findChannel(source.device_uid, source.channel_name, source.channel_metric),
            )
            .filter((channel): channel is AvailableChannel => channel != null)
            .map(channelKey)
    }
    const deviceUID = route.query.device as string | undefined
    const channelName = route.query.channel as string | undefined
    const channelMetric = route.query.metric as ChannelMetric | undefined
    if (deviceUID == null || channelName == null || channelMetric == null) return []
    const match = findChannel(deviceUID, channelName, channelMetric)
    return match != null ? [channelKey(match)] : []
}
chosenChannelKeys.value = startingChannelKeys()
// Create-from-sensor convenience: honor optional min/max/name query overrides
// (the fan "fail alert" prefills min=1 to catch 0 rpm).
if (shouldCreateAlert) {
    if (route.query.min != null) chosenMin.value = Number(route.query.min)
    if (route.query.max != null) chosenMax.value = Number(route.query.max)
    if (route.query.name != null) chosenName.value = String(route.query.name)
}

const saveAlert = async (): Promise<void> => {
    alert.max = chosenMax.value
    alert.min = chosenMin.value
    alert.name = chosenName.value
    alert.warmup_duration = chosenWarmupDuration.value
    alert.cooldown_duration = chosenCooldownDuration.value
    alert.repeat_interval = chosenRepeatMinutes.value * 60
    alert.enabled = chosenEnabled.value
    alert.desktop_notify = chosenDesktopNotification.value
    alert.desktop_notify_recovery = chosenDesktopNotificationRecovery.value
    alert.desktop_notify_audio = chosenDesktopNotificationAudio.value
    alert.shutdown_on_activation = chosenShutdownOnActivation.value
    alert.channel_sources = selectedChannels.value.map(
        (channel) => new ChannelSource(channel.deviceUID, channel.channelName, channel.metric),
    )
    alert.channel_source = alert.channel_sources[0]
    if (shouldCreateAlert) {
        const successful = await settingsStore.createAlert(alert)
        if (successful) {
            await settingsStore.loadAlertsAndLogs()
            contextIsDirty.value = false
            await router.push({ name: 'monitoring-alert', params: { alertUID: alert.uid } })
        }
    } else {
        const successful = await settingsStore.updateAlert(alert.uid)
        if (successful) contextIsDirty.value = false
    }
}

const duplicateAlert = async (): Promise<void> => {
    const copy = new Alert(
        `${alert.name} ${t('common.copy')}`,
        alertSources(alert).map(
            (source) =>
                new ChannelSource(source.device_uid, source.channel_name, source.channel_metric),
        ),
        alert.min,
        alert.max,
        alert.warmup_duration,
    )
    copy.cooldown_duration = alert.cooldown_duration
    copy.repeat_interval = alert.repeat_interval
    copy.enabled = alert.enabled
    copy.desktop_notify = alert.desktop_notify
    copy.desktop_notify_recovery = alert.desktop_notify_recovery
    copy.desktop_notify_audio = alert.desktop_notify_audio
    copy.shutdown_on_activation = alert.shutdown_on_activation
    const successful = await settingsStore.createAlert(copy)
    if (successful) {
        await settingsStore.loadAlertsAndLogs()
        await router.push({ name: 'monitoring-alert', params: { alertUID: copy.uid } })
    }
}

const deleteAlert = (): void => {
    confirm.require({
        message: t('views.alerts.deleteAlertConfirm', { name: alert.name }),
        header: t('views.alerts.deleteAlert'),
        icon: mdiAlertOutline,
        accept: async () => {
            const successful = await settingsStore.deleteAlert(alert.uid)
            if (successful) {
                contextIsDirty.value = false
                await router.push({ name: 'monitoring-alerts' })
            }
        },
    })
}

const saveNameFunction = async (newName: string): Promise<boolean> => {
    if (newName.length > 0) {
        alert.name = newName
        const successful = await settingsStore.updateAlert(alert.uid)
        if (successful) {
            const isAlreadyDirty = contextIsDirty.value
            chosenName.value = newName
            if (!isAlreadyDirty) {
                setTimeout(() => (contextIsDirty.value = false))
            }
            return true
        } else {
            alert.name = chosenName.value
            return false
        }
    }
    return false
}

const silencedUntilText = (): string =>
    new Date(alert.silenced_until!).toLocaleString([], {
        day: 'numeric',
        month: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    })

const updateValues = (): void => {
    for (const channelDevice of channelSources.value) {
        for (const channel of channelDevice.channels) {
            switch (channel.metric) {
                case ChannelMetric.Duty:
                case ChannelMetric.Load:
                    channel.value =
                        deviceStore.currentDeviceStatus
                            .get(channel.deviceUID)!
                            .get(channel.channelName)!.duty || '0'
                    break
                case ChannelMetric.RPM:
                    channel.value =
                        deviceStore.currentDeviceStatus
                            .get(channel.deviceUID)!
                            .get(channel.channelName)!.rpm || '0'
                    break
                case ChannelMetric.Freq:
                    channel.value =
                        deviceStore.currentDeviceStatus
                            .get(channel.deviceUID)!
                            .get(channel.channelName)!.freq || '0'
                    break
                case ChannelMetric.Temp:
                default:
                    channel.value =
                        deviceStore.currentDeviceStatus
                            .get(channel.deviceUID)!
                            .get(channel.channelName)!.temp || '0.0'
                    break
            }
        }
    }
}

const selectedChannels = computed<Array<AvailableChannel>>(() =>
    chosenChannelKeys.value
        .map((key) =>
            channelSources.value
                .flatMap((source) => source.channels)
                .find((candidate) => channelKey(candidate) === key),
        )
        .filter((channel): channel is AvailableChannel => channel != null),
)
// All sources share one metric; the first pick establishes it.
const selectedMetric = computed<ChannelMetric | undefined>(() => selectedChannels.value[0]?.metric)

const liveReadings = (): Array<number> =>
    selectedChannels.value.map((channel) => Number(channel.value))
// A threshold or reading already above the locked range comes up unlocked, so opening an
// alert never silently pulls its thresholds back in.
const thresholdsUnlocked: Ref<boolean> = ref(
    needsUnlock(selectedMetric.value, [chosenMax.value, chosenMin.value, ...liveReadings()]),
)
const rangeUnlocked = computed(
    () => hasThresholdLock(selectedMetric.value) && thresholdsUnlocked.value,
)
const maxLimit = computed(() => thresholdMax(selectedMetric.value, rangeUnlocked.value))
const lockedMaxLimit = computed(() =>
    hasThresholdLock(selectedMetric.value) ? thresholdMax(selectedMetric.value, false) : undefined,
)
const step = computed(() => thresholdStep(selectedMetric.value))
// Re-locking pulls out-of-range thresholds back in, so the lock and the values never disagree.
watch(thresholdsUnlocked, (unlocked) => {
    if (unlocked || !hasThresholdLock(selectedMetric.value)) return
    chosenMax.value = Math.min(chosenMax.value, maxLimit.value)
    chosenMin.value = Math.min(chosenMin.value, chosenMax.value - THRESHOLD_GAP)
})
// A preselected RPM source without a "Greater Than" starts at the top of its range.
if (shouldCreateAlert && route.query.max == null && selectedMetric.value === ChannelMetric.RPM) {
    chosenMax.value = maxLimit.value
}

const sourceGroups = computed(() =>
    channelSources.value.map((source) => ({
        label: source.deviceName,
        options: source.channels.map((channel) => ({
            label: channel.channelFrontendName,
            value: channelKey(channel),
            color: channel.lineColor,
            rightText: `${groupDigits(channel.value)}${valueSuffix(channel.metric)}`,
            disabled: selectedMetric.value != null && channel.metric !== selectedMetric.value,
        })),
    })),
)
const referenceThresholds = computed<Thresholds>(() => ({
    min: chosenMin.value,
    max: chosenMax.value,
    ceiling: thresholdMax(selectedMetric.value, true),
}))
// A reference value picked as a threshold. One above the locked range unlocks it: the pick
// is as deliberate as the lock button.
const applyThreshold = (target: ThresholdTarget, value: number): void => {
    if (needsUnlock(selectedMetric.value, [value])) thresholdsUnlocked.value = true
    if (target === 'max') chosenMax.value = value
    else chosenMin.value = value
}
const onSourcesChange = (value: string | string[] | undefined): void => {
    if (!Array.isArray(value)) return
    const hadNone = chosenChannelKeys.value.length === 0
    chosenChannelKeys.value = value
    // A first pick starts locked. Any pick that reads above the locked range unlocks it.
    if (hadNone) thresholdsUnlocked.value = false
    if (needsUnlock(selectedMetric.value, liveReadings())) thresholdsUnlocked.value = true
    if (hadNone && selectedMetric.value != null) {
        chosenMax.value = maxLimit.value
        if (chosenMin.value >= chosenMax.value) chosenMin.value = defaultMin
    }
}

const valueSuffix = (metric: ChannelMetric | undefined): string => {
    switch (metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return ` ${t('common.percentUnit')}`
        case ChannelMetric.RPM:
            return ` ${t('common.rpmAbbr')}`
        case ChannelMetric.Freq:
            return ` ${t('common.mhzAbbr')}`
        case ChannelMetric.Temp:
        default:
            return ` ${t('common.tempUnit')}`
    }
}

const checkForUnsavedChanges = (): boolean | Promise<boolean> => {
    if (!contextIsDirty.value) {
        return true
    }
    return new Promise<boolean>((resolve) => {
        confirm.require({
            message: t('views.alerts.unsavedChanges'),
            header: t('views.alerts.unsavedChangesHeader'),
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

onMounted(async () => {
    watch(rawStore.currentDeviceStatus, () => {
        updateValues()
    })
    watch(settingsStore.allUIDeviceSettings, async () => {
        await fillChannelSources()
    })
    watch(
        [
            chosenChannelKeys,
            chosenMax,
            chosenMin,
            chosenName,
            chosenWarmupDuration,
            chosenCooldownDuration,
            chosenRepeatMinutes,
            chosenEnabled,
            chosenDesktopNotification,
            chosenDesktopNotificationRecovery,
            chosenDesktopNotificationAudio,
            chosenShutdownOnActivation,
        ],
        () => {
            contextIsDirty.value = true
        },
    )
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)
})
</script>

<template>
    <div class="flex h-full flex-col">
        <entity-page-header>
            <template #title>
                <entity-title-rename
                    :current-name="chosenName"
                    :save-name-function="saveNameFunction"
                />
            </template>
            <template #actions>
                <UiButton
                    v-if="!shouldCreateAlert"
                    variant="ghost"
                    size="icon"
                    v-tooltip.top="t('views.alerts.duplicateAlert')"
                    @click="duplicateAlert"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiContentDuplicate"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                </UiButton>
                <UiButton
                    v-if="!shouldCreateAlert"
                    variant="ghost"
                    size="icon"
                    v-tooltip.top="t('views.alerts.deleteAlert')"
                    @click="deleteAlert"
                >
                    <svg-icon
                        type="mdi"
                        :path="mdiTrashCanOutline"
                        :size="deviceStore.getREMSize(1.25)"
                    />
                </UiButton>
                <div class="p-2">
                    <UiButton
                        class="w-32"
                        :class="{ 'animate-pulse-fast': contextIsDirty }"
                        v-tooltip.top="t('views.alerts.saveAlert')"
                        :disabled="chosenChannelKeys.length === 0 || chosenName.length === 0"
                        @click="saveAlert"
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
        </entity-page-header>
        <ScrollAreaRoot class="min-h-0 flex-1" style="--scrollbar-size: 10px">
            <ScrollAreaViewport class="p-4 h-full w-full">
                <div class="flex flex-col-reverse items-start lg:flex-row mt-0 w-full">
                    <!-- Stacked (below lg) the listbox needs its own height: the
                         flex-1/basis-0 fill only works next to the settings column. -->
                    <div
                        class="flex w-full flex-col self-stretch mt-4 lg:mt-0 lg:mr-4 lg:w-96 lg:shrink-0"
                    >
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            {{ t('views.alerts.channelSources') }}
                        </small>
                        <UiGroupedListbox
                            :model-value="chosenChannelKeys"
                            multiple
                            class="mt-1 h-96 min-h-0 lg:h-auto lg:flex-1 lg:basis-0"
                            :groups="sourceGroups"
                            filter
                            :filter-placeholder="t('common.search')"
                            :invalid="chosenChannelKeys.length === 0"
                            v-tooltip.top="t('views.alerts.channelSourcesTooltip')"
                            @update:model-value="onSourcesChange"
                        />
                    </div>
                    <!-- Responsive card grid: full-width cards when narrow, two
                         columns when wide, like the overview pages. -->
                    <div
                        class="grid w-full max-w-4xl flex-1 grid-cols-1 items-start gap-4 xl:grid-cols-2"
                    >
                        <UiSettingsCard :title="t('views.alerts.sectionGeneral')">
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.enabledTooltip')"
                                :label="t('views.alerts.enabled')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiSwitch v-model="chosenEnabled" />
                                </div>
                            </UiSettingRow>
                            <UiSettingRow
                                v-if="!shouldCreateAlert"
                                v-tooltip.top="t('views.alerts.silenceTooltip')"
                                :label="t('views.alerts.silence')"
                            >
                                <div class="flex items-center justify-end gap-2">
                                    <UiTag
                                        v-if="alertIsSilenced(alert)"
                                        :value="
                                            t('views.alerts.silencedUntil', {
                                                time: silencedUntilText(),
                                            })
                                        "
                                        severity="warn"
                                    />
                                    <AlertSilenceMenu :alert="alert">
                                        <template #trigger>
                                            <UiButton variant="ghost" size="icon">
                                                <svg-icon
                                                    type="mdi"
                                                    :path="mdiBellSleepOutline"
                                                    :size="deviceStore.getREMSize(1.25)"
                                                />
                                            </UiButton>
                                        </template>
                                    </AlertSilenceMenu>
                                </div>
                            </UiSettingRow>
                        </UiSettingsCard>
                        <UiSettingsCard :title="t('views.alerts.triggerConditions')">
                            <AlertSourceReference
                                v-if="selectedChannels.length > 0"
                                :sources="selectedChannels"
                                :metric="selectedMetric"
                                :thresholds="referenceThresholds"
                                @apply="applyThreshold"
                            />
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.maxValueTooltip')"
                                :label="t('views.alerts.greaterThan')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <div class="flex items-center gap-1">
                                        <UiLockToggle
                                            v-if="hasThresholdLock(selectedMetric)"
                                            v-model="thresholdsUnlocked"
                                            v-tooltip.top="
                                                thresholdsUnlocked
                                                    ? t('layout.settings.tooltips.lockRange')
                                                    : t('layout.settings.tooltips.unlockRange')
                                            "
                                        />
                                        <UiNumberInput
                                            v-model="chosenMax"
                                            :min="chosenMin + THRESHOLD_GAP"
                                            :max="maxLimit"
                                            :safe-max="lockedMaxLimit"
                                            grouped
                                            :step="step"
                                            :suffix="valueSuffix(selectedMetric)"
                                            :disabled="selectedMetric == null"
                                        />
                                    </div>
                                    <!-- The unlocked range is too wide for a slider. -->
                                    <UiSlider
                                        v-if="!rangeUnlocked"
                                        v-model="chosenMax"
                                        class="!w-48"
                                        :step="step"
                                        :min="alignUpToStep(chosenMin + THRESHOLD_GAP, step)"
                                        :max="maxLimit"
                                        :disabled="selectedMetric == null"
                                    />
                                </div>
                            </UiSettingRow>
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.minValueTooltip')"
                                :label="t('views.alerts.lessThan')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiNumberInput
                                        v-model="chosenMin"
                                        :min="0"
                                        :max="chosenMax - THRESHOLD_GAP"
                                        :safe-max="lockedMaxLimit"
                                        grouped
                                        :step="step"
                                        :suffix="valueSuffix(selectedMetric)"
                                        :disabled="selectedMetric == null"
                                    />
                                    <UiSlider
                                        v-if="!rangeUnlocked"
                                        v-model="chosenMin"
                                        class="!w-48"
                                        :step="step"
                                        :min="0"
                                        :max="chosenMax - THRESHOLD_GAP"
                                        :disabled="selectedMetric == null"
                                    />
                                </div>
                            </UiSettingRow>
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.warmupDurationTooltip')"
                                :label="t('views.alerts.warmupGreaterThan')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiNumberInput
                                        v-model="chosenWarmupDuration"
                                        :min="0"
                                        :max="60"
                                        :step="0.5"
                                        :suffix="' s'"
                                        :disabled="selectedMetric == null"
                                    />
                                    <UiSlider
                                        v-model="chosenWarmupDuration"
                                        class="!w-48"
                                        :step="0.5"
                                        :min="0"
                                        :max="60"
                                        :disabled="selectedMetric == null"
                                    />
                                </div>
                            </UiSettingRow>
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.cooldownDurationTooltip')"
                                :label="t('views.alerts.cooldownLessThan')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiNumberInput
                                        v-model="chosenCooldownDuration"
                                        :min="0"
                                        :max="60"
                                        :step="0.5"
                                        :suffix="' s'"
                                        :disabled="selectedMetric == null"
                                    />
                                    <UiSlider
                                        v-model="chosenCooldownDuration"
                                        class="!w-48"
                                        :step="0.5"
                                        :min="0"
                                        :max="60"
                                        :disabled="selectedMetric == null"
                                    />
                                </div>
                            </UiSettingRow>
                        </UiSettingsCard>
                        <UiSettingsCard :title="t('views.alerts.sectionNotifications')">
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.desktopNotifyTooltip')"
                                :label="t('views.alerts.desktopNotify')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiSwitch v-model="chosenDesktopNotification" />
                                </div>
                            </UiSettingRow>
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.desktopNotifyRecoveryTooltip')"
                                :label="t('views.alerts.desktopNotifyRecovery')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiSwitch
                                        v-model="chosenDesktopNotificationRecovery"
                                        :disabled="!chosenDesktopNotification"
                                    />
                                </div>
                            </UiSettingRow>
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.desktopNotifyAudioTooltip')"
                                :label="t('views.alerts.desktopNotifyAudio')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiSwitch
                                        v-model="chosenDesktopNotificationAudio"
                                        :disabled="!chosenDesktopNotification"
                                    />
                                </div>
                            </UiSettingRow>
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.repeatIntervalTooltip')"
                                :label="t('views.alerts.repeatInterval')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiNumberInput
                                        v-model="chosenRepeatMinutes"
                                        :min="0"
                                        :max="120"
                                        :step="1"
                                        :suffix="' min'"
                                        :disabled="!chosenDesktopNotification"
                                    />
                                </div>
                            </UiSettingRow>
                        </UiSettingsCard>
                        <UiSettingsCard :title="t('views.alerts.sectionActions')">
                            <UiSettingRow
                                v-tooltip.top="t('views.alerts.shutdownOnActivationTooltip')"
                                :label="t('views.alerts.shutdownOnActivation')"
                            >
                                <div class="flex flex-col items-end gap-2">
                                    <UiSwitch v-model="chosenShutdownOnActivation" />
                                </div>
                            </UiSettingRow>
                        </UiSettingsCard>
                    </div>
                </div>
                <div v-if="!shouldCreateAlert" class="mt-8 flex max-w-4xl flex-col">
                    <span class="pb-3 ml-1 font-semibold text-xl text-text-color">{{
                        t('views.alerts.alertLogs')
                    }}</span>
                    <AlertLogTable :alert-u-i-d="props.alertUID" />
                </div>
            </ScrollAreaViewport>
            <ScrollAreaScrollbar
                class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
                orientation="vertical"
            >
                <ScrollAreaThumb
                    class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
                />
            </ScrollAreaScrollbar>
        </ScrollAreaRoot>
    </div>
</template>

<style scoped lang="scss"></style>
