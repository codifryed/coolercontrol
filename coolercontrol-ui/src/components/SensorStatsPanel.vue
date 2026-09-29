<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiArrowTopRightBottomLeft, mdiClose, mdiRefresh } from '@mdi/js'
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { DataType, getLocalizedDataType } from '@/models/Dashboard.ts'
import type { UID } from '@/models/Device.ts'
import type { ChannelAttribute } from '@/models/ChannelAttributes.ts'
import type { ChannelStats } from '@/models/Stats.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useCalibrationStore } from '@/stores/CalibrationStore.ts'
import { useLifetimeStats } from '@/composables/useLifetimeStats.ts'
import { useStatFormat } from '@/composables/useStatFormat.ts'
import {
    formatSpan,
    lifetimeStatsOf,
    lifetimeToDisplay,
    type WindowLineStats,
    type WindowStatsPayload,
} from '@/components/chartStats.ts'
import {
    attributeLabel,
    formatAttributeValue,
    limitColor,
    type ThresholdLine,
} from '@/components/channelAttributes.ts'
import {
    channelDetail,
    edgeDecimals,
    STALL_POLLS_MIN,
    stallDutyMinOf,
    type LineDetail,
    type TimeInRange,
    type TimeInRangeBand,
} from '@/components/windowDetail.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import HelpIcon from '@/components/info/HelpIcon.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import UiToggleGroup, { type UiToggleOption } from '@/shell/ui/UiToggleGroup.vue'

interface Props {
    payload: WindowStatsPayload | null
    rangeMinutes: number
    attributes: Array<ChannelAttribute>
    // The limits drawn on the chart, matched to their rows by sysfs name.
    limitLines: Array<ThresholdLine>
}

const props = defineProps<Props>()
const emit = defineEmits<{ (e: 'refreshAttributes'): void }>()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const { t } = useI18n()
const { stats: lifetime } = useLifetimeStats()
const { formatStat, formatJitter, unitSuffix, windowLabelOf } = useStatFormat()

const lines = computed((): Array<WindowLineStats> => props.payload?.lines ?? [])
const colors = useThemeColorsStore()

interface AttributeMark {
    // The line colour when the attribute is drawn and on the chart.
    color: string | null
    // Drawn, but outside the scale's current range (a 110 °C limit on a 0 to 100 axis).
    offChart: boolean
}
const attributeMarks = computed((): Map<string, AttributeMark> => {
    const marks = new Map<string, AttributeMark>()
    for (const line of props.limitLines) {
        const range = props.payload?.scaleRanges[line.scale]
        const offChart = range != null && (line.value < range[0] || line.value > range[1])
        marks.set(line.name, {
            color: offChart ? null : limitColor(line.severity, colors.themeColors),
            offChart,
        })
    }
    return marks
})
const positionClasses = computed((): string =>
    settingsStore.sensorStatsPanelPosition === 'top-left'
        ? 'top-4 left-[4.5rem]'
        : 'bottom-12 right-[6rem]',
)
const windowLabel = computed((): string => windowLabelOf(props.payload, props.rangeMinutes))

const lifetimeOf = (line: WindowLineStats): ChannelStats | null =>
    lifetimeToDisplay(
        lifetimeStatsOf(lifetime.value, line),
        line.dataType,
        settingsStore.frequencyPrecision,
    )

const statRows = [
    { key: 'min', label: 'components.chartStats.min' },
    { key: 'max', label: 'components.chartStats.max' },
    { key: 'avg', label: 'components.chartStats.avg' },
] as const

const calibrationStore = useCalibrationStore()
const stallDutyMin = (deviceUID: UID, channelName: string): number => {
    const status = calibrationStore.statusFor(deviceUID, channelName)
    return stallDutyMinOf(status?.phase === 'completed' ? status.calibration : undefined)
}

// The behaviour rows, keyed by line name. They read the device's status history over the same
// window as the other stats, so they follow the time range and zoom.
const details = computed((): Map<string, LineDetail> => {
    const result = new Map<string, LineDetail>()
    const payload = props.payload
    if (payload == null) return result
    const channels = new Map<string, Array<WindowLineStats>>()
    for (const line of payload.lines) {
        const key = `${line.deviceUID}::${line.channelName}`
        const channelLines = channels.get(key)
        if (channelLines == null) {
            channels.set(key, [line])
        } else {
            channelLines.push(line)
        }
    }
    const devices = [...deviceStore.allDevices()]
    for (const channelLines of channels.values()) {
        const { deviceUID, channelName } = channelLines[0]
        const device = devices.find((d) => d.uid === deviceUID)
        if (device == null) continue
        const byType = channelDetail(
            device.status_history,
            channelName,
            channelLines.map((line) => line.dataType),
            payload.windowStart,
            payload.windowEnd,
            {
                precision: settingsStore.frequencyPrecision,
                pollSeconds: settingsStore.ccSettings.poll_rate,
                stallDutyMin: stallDutyMin(deviceUID, channelName),
            },
        )
        for (const line of channelLines) {
            const detail = byType.get(line.dataType)
            if (detail != null) result.set(line.lineName, detail)
        }
    }
    return result
})

const formatShare = (share: number | null | undefined): string => {
    if (share == null) return '-'
    const percent = Math.round(share * 100)
    const unit = t('common.percentUnit')
    return percent === 0 && share > 0 ? `<1${unit}` : `${percent}${unit}`
}
const formatChanges = (detail: LineDetail | undefined): string => {
    if (detail?.directionChanges == null) return '-'
    if (detail.directionChangesPerMinute == null) return String(detail.directionChanges)
    return t('components.statsPanel.changesValue', {
        count: detail.directionChanges,
        rate: detail.directionChangesPerMinute.toFixed(1),
    })
}
const formatStalls = (detail: LineDetail | undefined): string => {
    const stalls = detail?.stalls
    if (stalls == null) return '-'
    if (stalls.count === 0) return t('components.statsPanel.stallsNone')
    return t('components.statsPanel.stallsValue', {
        count: stalls.count,
        duration: formatSpan(stalls.seconds),
    })
}

// The panel shows one line at a time, which keeps a fan's list short; fans start on their speed.
const selectedLineName = ref<string>('')
const activeLine = computed(
    (): WindowLineStats | undefined =>
        lines.value.find((line) => line.lineName === selectedLineName.value) ??
        lines.value.find((line) => line.dataType === DataType.RPM) ??
        lines.value[0],
)
const shownLines = computed((): Array<WindowLineStats> =>
    activeLine.value == null ? [] : [activeLine.value],
)
const lineOptions = computed((): Array<UiToggleOption> =>
    lines.value.map((line) => ({
        label: getLocalizedDataType(line.dataType),
        value: line.lineName,
    })),
)
const tir = computed((): TimeInRange | null =>
    activeLine.value == null
        ? null
        : (details.value.get(activeLine.value.lineName)?.timeInRange ?? null),
)
const formatBand = (band: TimeInRangeBand, width: number, dataType: DataType): string => {
    const decimals = edgeDecimals(width)
    const range = t('components.statsPanel.band', {
        from: band.from.toFixed(decimals),
        to: band.to.toFixed(decimals),
    })
    return range + unitSuffix(dataType)
}

const movePanel = (): void => {
    settingsStore.sensorStatsPanelPosition =
        settingsStore.sensorStatsPanelPosition === 'top-left' ? 'bottom-right' : 'top-left'
}
</script>

<template>
    <div
        class="absolute z-10 max-h-[calc(100%-2rem)] w-[21rem] max-w-[calc(100%-6rem)] overflow-y-auto rounded-lg border border-border-one bg-bg-two text-sm shadow-lg"
        :class="positionClasses"
    >
        <div
            class="sticky top-0 z-10 flex items-center gap-1 border-b border-border-one bg-bg-two py-1 pl-3 pr-1"
        >
            <span class="flex-1 font-semibold">{{ t('components.chartStats.title') }}</span>
            <UiToggleGroup
                v-if="activeLine != null && lineOptions.length > 1"
                size="sm"
                class="mr-1"
                :aria-label="t('components.statsPanel.line')"
                :model-value="activeLine.lineName"
                :options="lineOptions"
                @update:model-value="selectedLineName = $event"
            />
            <UiButton
                variant="ghost"
                size="icon"
                class="!h-7 !w-7"
                :aria-label="t('components.statsPanel.moveCorner')"
                v-tooltip.top="t('components.statsPanel.moveCorner')"
                @click="movePanel"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiArrowTopRightBottomLeft"
                    :size="deviceStore.getREMSize(1)"
                />
            </UiButton>
            <UiButton
                variant="ghost"
                size="icon"
                class="!h-7 !w-7"
                :aria-label="t('components.statsPanel.hide')"
                v-tooltip.top="t('components.statsPanel.hide')"
                @click="settingsStore.sensorStatsPanelVisible = false"
            >
                <svg-icon type="mdi" :path="mdiClose" :size="deviceStore.getREMSize(1)" />
            </UiButton>
        </div>
        <table class="w-full tabular-nums">
            <thead>
                <tr class="text-xs text-text-color-secondary">
                    <th class="w-[40%] pb-0.5 pl-3 pr-2 pt-1.5"></th>
                    <th class="px-2 pb-0.5 pt-1.5 text-right font-medium">{{ windowLabel }}</th>
                    <th class="pl-2 pr-3 pb-0.5 pt-1.5 text-right font-medium">
                        <span class="inline-flex items-center gap-1">
                            {{ t('components.chartStats.sinceStart') }}
                            <HelpIcon
                                :text="t('components.chartStats.sinceStartHelp')"
                                :size="0.9"
                            />
                        </span>
                    </th>
                </tr>
            </thead>
            <tbody v-for="line in shownLines" :key="line.lineName">
                <tr>
                    <th
                        class="hyphens-auto pl-3 pr-2 py-0.5 text-left font-normal text-text-color-secondary"
                    >
                        {{ t('components.chartStats.now') }}
                    </th>
                    <td colspan="2" class="px-3 py-0.5 text-center font-semibold">
                        {{ formatStat(line.latest, line.dataType) }}
                    </td>
                </tr>
                <tr v-for="row in statRows" :key="row.key">
                    <th
                        class="hyphens-auto pl-3 pr-2 py-0.5 text-left font-normal text-text-color-secondary"
                    >
                        {{ t(row.label) }}
                    </th>
                    <td class="whitespace-nowrap px-2 py-0.5 text-right">
                        {{ formatStat(line.stats?.[row.key], line.dataType) }}
                    </td>
                    <td class="whitespace-nowrap pl-2 pr-3 py-0.5 text-right">
                        {{ formatStat(lifetimeOf(line)?.[row.key], line.dataType) }}
                    </td>
                </tr>
                <tr>
                    <th
                        class="hyphens-auto pl-3 pr-2 py-0.5 text-left font-normal text-text-color-secondary"
                    >
                        <span class="inline-flex items-center gap-1">
                            {{ t('components.statsPanel.jitter') }}
                            <HelpIcon :text="t('components.statsPanel.jitterHelp')" :size="0.9" />
                        </span>
                    </th>
                    <td class="whitespace-nowrap px-2 py-0.5 text-right">
                        {{ formatJitter(line.stats?.jitter, line.dataType) }}
                    </td>
                    <td
                        class="whitespace-nowrap pl-2 pr-3 py-0.5 text-right text-text-color-secondary"
                    >
                        -
                    </td>
                </tr>
                <tr v-if="line.dataType === DataType.DUTY">
                    <th
                        class="hyphens-auto pl-3 pr-2 py-0.5 text-left font-normal text-text-color-secondary"
                    >
                        <span class="inline-flex items-center gap-1">
                            {{ t('components.statsPanel.directionChanges') }}
                            <HelpIcon
                                :text="t('components.statsPanel.directionChangesHelp')"
                                :size="0.9"
                            />
                        </span>
                    </th>
                    <td class="whitespace-nowrap px-2 py-0.5 text-right">
                        {{ formatChanges(details.get(line.lineName)) }}
                    </td>
                    <td
                        class="whitespace-nowrap pl-2 pr-3 py-0.5 text-right text-text-color-secondary"
                    >
                        -
                    </td>
                </tr>
                <template v-if="line.dataType === DataType.RPM">
                    <tr>
                        <th
                            class="hyphens-auto pl-3 pr-2 py-0.5 text-left font-normal text-text-color-secondary"
                        >
                            <span class="inline-flex items-center gap-1">
                                {{ t('components.statsPanel.stopped') }}
                                <HelpIcon
                                    :text="t('components.statsPanel.stoppedHelp')"
                                    :size="0.9"
                                />
                            </span>
                        </th>
                        <td class="whitespace-nowrap px-2 py-0.5 text-right">
                            {{ formatShare(details.get(line.lineName)?.stoppedShare) }}
                        </td>
                        <td
                            class="whitespace-nowrap pl-2 pr-3 py-0.5 text-right text-text-color-secondary"
                        >
                            -
                        </td>
                    </tr>
                    <tr v-if="details.get(line.lineName)?.stalls != null">
                        <th
                            class="hyphens-auto pl-3 pr-2 py-0.5 text-left font-normal text-text-color-secondary"
                        >
                            <span class="inline-flex items-center gap-1">
                                {{ t('components.statsPanel.stalls') }}
                                <HelpIcon
                                    :text="
                                        t('components.statsPanel.stallsHelp', {
                                            polls: STALL_POLLS_MIN,
                                            duty: stallDutyMin(line.deviceUID, line.channelName),
                                        })
                                    "
                                    :size="0.9"
                                />
                            </span>
                        </th>
                        <td
                            class="whitespace-nowrap px-2 py-0.5 text-right"
                            :class="{
                                'font-semibold text-warning':
                                    (details.get(line.lineName)?.stalls?.count ?? 0) > 0,
                            }"
                        >
                            {{ formatStalls(details.get(line.lineName)) }}
                        </td>
                        <td
                            class="whitespace-nowrap pl-2 pr-3 py-0.5 text-right text-text-color-secondary"
                        >
                            -
                        </td>
                    </tr>
                </template>
            </tbody>
        </table>
        <div v-if="activeLine != null" class="mt-1 border-t border-border-one">
            <div class="flex items-center gap-1 py-1 pl-3 pr-1">
                <span class="font-semibold">{{ t('components.statsPanel.timeInRange') }}</span>
                <HelpIcon :text="t('components.statsPanel.timeInRangeHelp')" :size="0.9" />
            </div>
            <div
                v-if="tir != null"
                class="grid grid-cols-[max-content_1fr_max-content] items-center gap-x-2 gap-y-0.5 px-3 pb-2 tabular-nums"
            >
                <template v-for="band in tir.bands" :key="band.from">
                    <span class="whitespace-nowrap" :class="{ 'font-semibold': band.current }">
                        {{ formatBand(band, tir.width, activeLine.dataType) }}
                    </span>
                    <span
                        class="h-2 min-w-0.5 rounded-r-sm"
                        :style="{
                            width: `${(band.share / tir.bands[0].share) * 100}%`,
                            backgroundColor: activeLine.color,
                            opacity: band.current ? 1 : 0.45,
                        }"
                    ></span>
                    <span class="text-right" :class="{ 'font-semibold': band.current }">
                        {{ formatShare(band.share) }}
                    </span>
                </template>
                <template v-if="tir.rest >= 0.005">
                    <span class="text-text-color-secondary">
                        {{ t('components.statsPanel.otherBands') }}
                    </span>
                    <span></span>
                    <span class="text-right text-text-color-secondary">
                        {{ formatShare(tir.rest) }}
                    </span>
                </template>
            </div>
            <p v-else class="px-3 pb-2 text-text-color-secondary">
                {{ t('components.statsPanel.noReadings') }}
            </p>
        </div>
        <div class="mt-1 border-t border-border-one">
            <div class="flex items-center gap-1 py-1 pl-3 pr-1">
                <span class="font-semibold">{{ t('components.channelAttributes.title') }}</span>
                <HelpIcon :text="t('components.channelAttributes.help')" :size="0.9" />
                <span class="flex-1"></span>
                <UiButton
                    variant="ghost"
                    size="icon"
                    class="!h-7 !w-7"
                    :aria-label="t('components.channelAttributes.refresh')"
                    v-tooltip.top="t('components.channelAttributes.refresh')"
                    @click="emit('refreshAttributes')"
                >
                    <svg-icon type="mdi" :path="mdiRefresh" :size="deviceStore.getREMSize(1)" />
                </UiButton>
            </div>
            <table v-if="attributes.length > 0" class="mb-1.5 w-full tabular-nums">
                <tbody>
                    <tr v-for="attribute in attributes" :key="attribute.name">
                        <th class="px-3 py-0.5 text-left align-top font-normal leading-tight">
                            <div>{{ attributeLabel(attribute.kind, t) }}</div>
                            <code class="text-xs text-text-color-secondary">{{
                                attribute.name
                            }}</code>
                        </th>
                        <td class="px-3 py-0.5 text-right align-top leading-tight">
                            <div class="whitespace-nowrap">
                                <span
                                    v-if="attributeMarks.get(attribute.name)?.color"
                                    class="mr-1.5 inline-block w-3.5 border-t-2 border-dashed align-middle"
                                    :style="{
                                        borderColor: attributeMarks.get(attribute.name)!.color!,
                                    }"
                                ></span>
                                {{ formatAttributeValue(attribute, t) }}
                            </div>
                            <span
                                v-if="attributeMarks.get(attribute.name)?.offChart"
                                class="rounded border border-border-one px-1 text-xs text-text-color-secondary"
                                >{{ t('components.channelAttributes.offChart') }}</span
                            >
                        </td>
                    </tr>
                </tbody>
            </table>
            <p v-else class="px-3 pb-2 text-text-color-secondary">
                {{ t('components.channelAttributes.empty') }}
            </p>
        </div>
    </div>
</template>
