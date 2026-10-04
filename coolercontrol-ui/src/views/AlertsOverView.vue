<!--
  SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiBellPlusOutline,
    mdiBellSleepOutline,
    mdiMinusThick,
    mdiMonitor,
    mdiPower,
    mdiVolumeHigh,
} from '@mdi/js'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'reka-ui'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import {
    Alert,
    alertIsSilenced,
    alertSources,
    AlertState,
    getAlertStateDisplayName,
    getAlertStateIcon,
} from '@/models/Alert.ts'
import {
    ChannelMetric,
    ChannelSource,
    getChannelMetricDisplayName,
} from '@/models/ChannelSource.ts'
import UiTag from '@/shell/ui/UiTag.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import UiSwitch from '@/shell/ui/UiSwitch.vue'
import AlertSilenceMenu from '@/components/AlertSilenceMenu.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { computed } from 'vue'
import AlertLogTable from '@/components/AlertLogTable.vue'
import { groupDigits } from '@/shell/digitGroups.ts'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { getREMSize } = useDeviceStore()
const router = useRouter()
const { t } = useI18n()

const alertsList = computed(() => {
    const alerts = []
    for (const alert of settingsStore.alerts) {
        alerts.push(alert)
    }
    const alertMenuOrder = settingsStore.menuOrder.find((item) => item.id === 'alerts')
    if (alertMenuOrder?.children?.length) {
        const getIndex = (item: any) => {
            const index = alertMenuOrder.children.indexOf(item.uid)
            return index >= 0 ? index : Number.MAX_SAFE_INTEGER
        }
        alerts.sort((a: any, b: any) => getIndex(a) - getIndex(b))
    }
    return alerts
})
const onRowSelect = (alertUID: string) => {
    router.push({ name: 'monitoring-alert', params: { alertUID } })
}

// Card grid: alerts needing attention first, disabled last, then the
// panel's menu order.
const sortedAlerts = computed(() => {
    const rank = (alert: Alert): number => {
        if (!alert.enabled) return 3
        return alert.state === AlertState.Active ? 0 : alert.state === AlertState.Error ? 1 : 2
    }
    return [...alertsList.value].sort((a, b) => rank(a) - rank(b))
})
const tagSeverity = (state?: AlertState): 'danger' | 'warn' | 'success' =>
    state === AlertState.Active ? 'danger' : state === AlertState.Error ? 'warn' : 'success'
const sourceChannel = (source: ChannelSource) =>
    settingsStore.allUIDeviceSettings
        .get(source.device_uid)
        ?.sensorsAndChannels.get(source.channel_name)
const sourceLabel = (source: ChannelSource): string =>
    sourceChannel(source)?.name ?? source.channel_name
const sourceColor = (source: ChannelSource): string =>
    sourceChannel(source)?.color ?? 'rgb(var(--colors-text-color-secondary))'
// Per-source state text color, parallel to channel_sources by index.
const sourceStateClass = (alert: Alert, index: number): string => {
    const state = alert.source_states[index]?.state
    if (state === AlertState.Active) return 'text-error'
    if (state === AlertState.Error) return 'text-warning'
    return ''
}
// An RPM source takes the unit its channel's label names.
const valueSuffix = (source: ChannelSource): string => {
    switch (source.channel_metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return ` ${t('common.percentUnit')}`
        case ChannelMetric.RPM:
            return ` ${settingsStore.rpmUnit(source.device_uid, source.channel_name)}`
        case ChannelMetric.Freq:
            return ` ${t('common.mhzAbbr')}`
        default:
            return ` ${t('common.tempUnit')}`
    }
}
const liveValue = (source: ChannelSource): string => {
    const values = deviceStore.currentDeviceStatus.get(source.device_uid)?.get(source.channel_name)
    if (values == null) return ''
    switch (source.channel_metric) {
        case ChannelMetric.Duty:
        case ChannelMetric.Load:
            return values.duty ?? ''
        case ChannelMetric.RPM:
            return values.rpm ?? ''
        case ChannelMetric.Freq:
            return values.freq ?? ''
        default:
            return values.temp ?? ''
    }
}
// Phrase the thresholds as the firing condition (the inverse of the OK
// range); min is validated >= 0, so a 0 floor cannot fire and is omitted.
const triggerText = (alert: Alert): string => {
    const unit = valueSuffix(alertSources(alert)[0])
    const max = groupDigits(alert.max)
    if (alert.min > 0) {
        return t('views.alerts.triggersOutside', { min: groupDigits(alert.min), max, unit })
    }
    return t('views.alerts.triggersAbove', { max, unit })
}
const silencedUntilText = (alert: Alert): string =>
    new Date(alert.silenced_until!).toLocaleString([], {
        day: 'numeric',
        month: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    })
// Latest log entry per alert: the moment it entered its current state.
const lastLogTimes = computed(() => {
    const latest = new Map<string, string>()
    for (const log of settingsStore.alertLogs) {
        const existing = latest.get(log.uid)
        if (existing == null || new Date(log.timestamp) > new Date(existing)) {
            latest.set(log.uid, log.timestamp)
        }
    }
    return latest
})
</script>

<template>
    <div class="flex h-full flex-col">
        <div class="flex shrink-0 flex-wrap items-center gap-3 px-4 pt-4">
            <h1 class="text-xl font-semibold text-text-color">
                {{ t('views.alerts.alertsOverview') }}
            </h1>
        </div>
        <ScrollAreaRoot class="min-h-0 flex-1" style="--scrollbar-size: 10px">
            <ScrollAreaViewport class="p-4 h-full w-full">
                <div class="mt-8 flex flex-col">
                    <span class="pb-3 ml-1 font-semibold text-xl text-text-color">{{
                        t('layout.topbar.alerts')
                    }}</span>
                    <div
                        class="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4"
                    >
                        <div
                            v-for="alert in sortedAlerts"
                            :key="alert.uid"
                            class="flex cursor-pointer flex-col gap-2 rounded-lg border bg-bg-two p-4 hover:bg-surface-hover"
                            :class="[
                                alert.enabled &&
                                (alert.state === AlertState.Active ||
                                    alert.state === AlertState.Error)
                                    ? 'border-error'
                                    : 'border-border-one',
                                { 'opacity-60': !alert.enabled },
                            ]"
                            @click="onRowSelect(alert.uid)"
                        >
                            <div class="flex items-center gap-2">
                                <!-- Shape encodes silenced, color keeps the live state:
                                     a red sleep bell means firing-but-muted. -->
                                <svg-icon
                                    type="mdi"
                                    class="shrink-0"
                                    :class="{
                                        'text-error':
                                            alert.enabled && alert.state === AlertState.Active,
                                    }"
                                    :path="
                                        alert.enabled && alertIsSilenced(alert)
                                            ? mdiBellSleepOutline
                                            : getAlertStateIcon(alert.state!)
                                    "
                                    :size="getREMSize(1.5)"
                                />
                                <span class="truncate text-base font-semibold text-text-color">
                                    {{ alert.name }}
                                </span>
                                <span class="ml-auto flex shrink-0 items-center gap-1.5">
                                    <UiTag
                                        v-if="alert.enabled && alertIsSilenced(alert)"
                                        :value="
                                            t('views.alerts.silencedUntil', {
                                                time: silencedUntilText(alert),
                                            })
                                        "
                                        severity="warn"
                                    />
                                    <UiTag
                                        v-if="!alert.enabled"
                                        :value="t('views.alerts.disabledLabel')"
                                    />
                                    <UiTag
                                        v-else
                                        :value="getAlertStateDisplayName(alert.state!)"
                                        :severity="tagSeverity(alert.state)"
                                    />
                                </span>
                            </div>
                            <div class="flex flex-col gap-1">
                                <div
                                    v-for="(source, index) in alertSources(alert)"
                                    :key="`${source.channel_name}/${source.channel_metric}`"
                                    class="flex items-center gap-2 text-base text-text-color"
                                >
                                    <svg-icon
                                        type="mdi"
                                        :path="mdiMinusThick"
                                        :size="14"
                                        class="shrink-0"
                                        :style="{ color: sourceColor(source) }"
                                    />
                                    <span class="truncate" :class="sourceStateClass(alert, index)">
                                        {{ sourceLabel(source) }}
                                    </span>
                                    <span
                                        v-if="liveValue(source) !== ''"
                                        class="ml-auto shrink-0 text-text-color-secondary"
                                    >
                                        {{ groupDigits(liveValue(source))
                                        }}{{ valueSuffix(source) }}
                                    </span>
                                </div>
                            </div>
                            <div class="text-sm text-text-color-secondary">
                                {{
                                    getChannelMetricDisplayName(
                                        alertSources(alert)[0].channel_metric,
                                    )
                                }}: {{ triggerText(alert) }}
                            </div>
                            <div
                                class="mt-auto flex items-center gap-2 text-sm text-text-color-secondary"
                            >
                                <!-- The latest state transition: when the alert entered its
                                     current state. Meaningless while disabled (not evaluated). -->
                                <span
                                    v-if="alert.enabled && lastLogTimes.get(alert.uid) != null"
                                    class="truncate"
                                >
                                    {{
                                        t('views.alerts.stateSince', {
                                            state: getAlertStateDisplayName(alert.state!),
                                            time: new Date(
                                                lastLogTimes.get(alert.uid)!,
                                            ).toLocaleString(),
                                        })
                                    }}
                                </span>
                                <span
                                    class="ml-auto flex shrink-0 items-center gap-1.5"
                                    @click.stop
                                >
                                    <svg-icon
                                        v-if="alert.desktop_notify"
                                        type="mdi"
                                        :path="mdiMonitor"
                                        :size="14"
                                        v-tooltip.top="t('views.alerts.desktopNotify')"
                                    />
                                    <svg-icon
                                        v-if="alert.desktop_notify_audio"
                                        type="mdi"
                                        :path="mdiVolumeHigh"
                                        :size="14"
                                        v-tooltip.top="t('views.alerts.desktopNotifyAudio')"
                                    />
                                    <svg-icon
                                        v-if="alert.shutdown_on_activation"
                                        type="mdi"
                                        class="text-error"
                                        :path="mdiPower"
                                        :size="14"
                                        v-tooltip.top="t('views.alerts.shutdownOnActivation')"
                                    />
                                    <AlertSilenceMenu v-if="alert.enabled" :alert="alert">
                                        <template #trigger>
                                            <UiButton
                                                variant="ghost"
                                                size="icon"
                                                class="h-6 w-6"
                                                v-tooltip.top="t('views.alerts.silenceTooltip')"
                                            >
                                                <svg-icon
                                                    type="mdi"
                                                    :path="mdiBellSleepOutline"
                                                    :size="16"
                                                />
                                            </UiButton>
                                        </template>
                                    </AlertSilenceMenu>
                                    <!-- Wrapper span: UiSwitch roots at a component, so the
                                         tooltip directive needs a plain element to attach to. -->
                                    <span v-tooltip.top="t('views.alerts.enabledTooltip')">
                                        <UiSwitch
                                            :model-value="alert.enabled"
                                            @update:model-value="
                                                (value: boolean) =>
                                                    settingsStore.setAlertEnabled(alert.uid, value)
                                            "
                                        />
                                    </span>
                                </span>
                            </div>
                        </div>
                        <div
                            class="flex min-h-[8rem] cursor-pointer items-center justify-center gap-2 rounded-lg border border-dashed border-border-one p-4 text-text-color-secondary hover:bg-surface-hover hover:text-text-color"
                            @click="router.push({ name: 'monitoring-alert-new' })"
                        >
                            <svg-icon type="mdi" :path="mdiBellPlusOutline" :size="20" />
                            {{ t('views.alerts.newAlert') }}
                        </div>
                    </div>
                </div>
                <div class="mt-8 flex flex-col">
                    <span class="pb-3 ml-1 font-semibold text-xl text-text-color">{{
                        t('views.alerts.alertLogs')
                    }}</span>
                    <AlertLogTable />
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
