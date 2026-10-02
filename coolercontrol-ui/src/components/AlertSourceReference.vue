<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// What the selected alert sources really read, and the limits their drivers report, so a
// threshold can be picked from real numbers rather than guessed.
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiMinusThick, mdiRefresh } from '@mdi/js'
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useLifetimeStats } from '@/composables/useLifetimeStats.ts'
import HelpIcon from '@/components/info/HelpIcon.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import AlertReferenceValue from '@/components/AlertReferenceValue.vue'
import type { ChannelAttribute } from '@/models/ChannelAttributes.ts'
import type { ChannelMetric } from '@/models/ChannelSource.ts'
import { formatSpan, formatStatValue } from '@/components/chartStats.ts'
import { windowValues } from '@/components/windowDetail.ts'
import { groupDigits } from '@/shell/digitGroups.ts'
import { attributeLabel, formatAttributeNumber } from '@/components/channelAttributes.ts'
import {
    metricDataType,
    metricHasDriverLimits,
    observedRange,
    runReachesWarmup,
    thresholdAttributes,
    timeOutside,
    type ReferenceSource,
    type ThresholdTarget,
    type Thresholds,
} from '@/components/alertReference.ts'

const props = defineProps<{
    sources: Array<ReferenceSource>
    metric: ChannelMetric | undefined
    thresholds: Thresholds
    // The alert's "Condition Triggered Longer Than".
    warmupSeconds: number
}>()
const emit = defineEmits<{ apply: [target: ThresholdTarget, value: number] }>()
const relayApply = (target: ThresholdTarget, value: number): void => emit('apply', target, value)

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { displayOf } = useLifetimeStats()

// Alert thresholds are in the daemon's own units, so no frequency precision applies.
const RAW_PRECISION = 1
const dataType = computed(() => (props.metric == null ? undefined : metricDataType(props.metric)))
const formatNumber = (value: number): string =>
    dataType.value == null ? String(value) : formatStatValue(value, dataType.value, RAW_PRECISION)

const attributeKey = (source: ReferenceSource): string =>
    `${source.deviceUID}/${source.channelName}`

// The card follows its own width, not the window's: the side panel and the source list
// take a varying share of it. With room, time outside gets columns that line up across the
// sources. Without, it moves to a line under each source.
const WIDE_REM_MIN = 38
const root = ref<HTMLElement>()
const wide = ref(false)
let resizeObserver: ResizeObserver | null = null
onMounted(() => {
    if (root.value == null) return
    resizeObserver = new ResizeObserver((entries) => {
        wide.value = entries[0].contentRect.width >= deviceStore.getREMSize(WIDE_REM_MIN)
    })
    resizeObserver.observe(root.value)
})

// Driver attributes per channel. Each one costs a device read, so a channel is read once,
// when it is first selected, and only one at a time.
const attributes = shallowRef(new Map<string, Array<ChannelAttribute>>())
const hasDriverLimits = computed(() => metricHasDriverLimits(props.metric))
let reading = false
let unmounted = false
onBeforeUnmount(() => {
    unmounted = true
    resizeObserver?.disconnect()
})
// More reads than this in one pass means the selection keeps changing under it.
const READS_PER_PASS_MAX = 64
const readMissing = async (): Promise<void> => {
    if (reading) return
    reading = true
    try {
        for (let reads = 0; reads < READS_PER_PASS_MAX; reads++) {
            if (unmounted || !hasDriverLimits.value) return
            const next = props.sources.find((source) => !attributes.value.has(attributeKey(source)))
            if (next == null) return
            const key = attributeKey(next)
            const result = await deviceStore.daemonClient.getChannelAttributes(
                next.deviceUID,
                next.channelName,
            )
            attributes.value = new Map(attributes.value).set(key, result)
        }
    } finally {
        reading = false
    }
}
const readAgain = (): void => {
    attributes.value = new Map()
    void readMissing()
}
watch(
    () => props.sources.map(attributeKey).join('|') + props.metric,
    () => void readMissing(),
    { immediate: true },
)

interface OutsideItem {
    key: string
    label: string
    text: string
    // Nothing was outside, so the item recedes.
    none: boolean
    // The stretch is long enough to have triggered the alert.
    triggers: boolean
}
interface Outside {
    minutes: number
    above: OutsideItem
    // Absent when the lower threshold is 0, which cannot be crossed.
    below: OutsideItem | null
    longest: OutsideItem
}
const outsideItems = (outside: Outside): Array<OutsideItem> =>
    [outside.above, outside.below, outside.longest].filter(
        (item): item is OutsideItem => item != null,
    )
const outsideClass = (item: OutsideItem | null) => ({
    'text-text-color-secondary': item == null || item.none,
    'font-semibold text-warning': item?.triggers === true,
})
const shareText = (count: number, readings: number): string => {
    const percent = (count / readings) * 100
    const shown = percent < 1 ? '<1' : percent.toFixed(0)
    return ` (${shown} ${t('common.percentUnit')})`
}
// How the source's recent readings sit against the thresholds as they are set right now.
// It covers the status history the UI holds, which is what the daemon keeps: the last hour.
const outsideOf = (source: ReferenceSource): Outside | null => {
    if (dataType.value == null) return null
    const device = [...deviceStore.allDevices()].find((d) => d.uid === source.deviceUID)
    if (device == null) return null
    const { min, max } = props.thresholds
    const measured = timeOutside(
        windowValues(device.status_history, source.channelName, dataType.value),
        min,
        max,
    )
    if (measured == null) return null
    const pollSeconds = settingsStore.ccSettings.poll_rate
    const span = (readings: number): string => formatSpan(readings * pollSeconds)
    const sideItem = (key: string, label: string, count: number): OutsideItem => ({
        key,
        label,
        text: span(count) + (count > 0 ? shareText(count, measured.readings) : ''),
        none: count === 0,
        triggers: false,
    })
    return {
        minutes: Math.max(1, Math.round((measured.readings * pollSeconds) / 60)),
        above: sideItem('above', t('views.alerts.above'), measured.above),
        below: min > 0 ? sideItem('below', t('views.alerts.below'), measured.below) : null,
        longest: {
            key: 'longest',
            label: t('views.alerts.longest'),
            text: span(measured.longestRun),
            none: measured.longestRun === 0,
            triggers: runReachesWarmup(measured.longestRun, pollSeconds, props.warmupSeconds),
        },
    }
}

// displayOf reads the lifetime stats, which change with every status update, so the rows
// and their time outside follow the readings as they arrive.
const rows = computed(() =>
    props.sources.map((source) => {
        const stats =
            dataType.value == null
                ? null
                : displayOf({
                      deviceUID: source.deviceUID,
                      channelName: source.channelName,
                      dataType: dataType.value,
                  })
        return {
            key: attributeKey(source),
            source,
            stats,
            limits: thresholdAttributes(
                attributes.value.get(attributeKey(source)) ?? [],
                props.metric,
            ),
            outside: outsideOf(source),
        }
    }),
)
// Only worth a row once there is more than one source to span.
const allSources = computed(() =>
    rows.value.length < 2 ? null : observedRange(rows.value.map((row) => row.stats)),
)
// Every device keeps the same span of history, so the longest any source covers names it.
const outsideMinutes = computed(() =>
    rows.value.reduce((minutes, row) => Math.max(minutes, row.outside?.minutes ?? 0), 0),
)
const columnCount = computed(() => (wide.value ? 8 : 5))
</script>

<template>
    <div ref="root" class="rounded-lg border border-border-one bg-bg-two">
        <div class="flex items-center gap-1.5 border-b border-border-one px-4 py-3">
            <span class="text-base font-semibold text-text-color">
                {{ t('views.alerts.reference') }}
            </span>
            <HelpIcon
                :text="`${t('views.alerts.referenceHelp')}\n${t('views.alerts.outsideHelp')}`"
            />
            <span class="flex-1"></span>
            <!-- Negative margin: the button must not make this title bar taller than the
                 other cards'. -->
            <UiButton
                v-if="hasDriverLimits && sources.length > 0"
                variant="ghost"
                size="icon"
                class="-my-1 !h-7 !w-7"
                :aria-label="t('components.channelAttributes.refresh')"
                v-tooltip.top="t('components.channelAttributes.refresh')"
                @click="readAgain"
            >
                <svg-icon type="mdi" :path="mdiRefresh" :size="deviceStore.getREMSize(1)" />
            </UiButton>
        </div>
        <p v-if="sources.length === 0" class="px-4 py-3 text-base text-text-color-secondary">
            {{ t('views.alerts.referenceEmpty') }}
        </p>
        <div v-else class="max-h-72 overflow-y-auto px-4 py-2">
            <table class="w-full text-sm tabular-nums text-text-color">
                <thead class="text-xs text-text-color-secondary">
                    <tr>
                        <th rowspan="2"></th>
                        <th rowspan="2" class="px-1.5 pb-0.5 text-right align-bottom font-medium">
                            {{ t('components.chartStats.now') }}
                        </th>
                        <th colspan="3" class="pl-1.5 text-right font-medium">
                            <span class="inline-flex items-center gap-1">
                                {{ t('components.chartStats.sinceStart') }}
                                <HelpIcon
                                    :text="t('components.chartStats.sinceStartHelp')"
                                    :size="0.9"
                                />
                            </span>
                        </th>
                        <th v-if="wide" colspan="3" class="pl-6 text-right font-medium">
                            <template v-if="outsideMinutes > 0">
                                {{
                                    t('components.chartStats.lastMinutes', {
                                        minutes: outsideMinutes,
                                    })
                                }}
                            </template>
                        </th>
                    </tr>
                    <tr>
                        <th class="px-1.5 pb-0.5 text-right font-medium">
                            {{ t('components.chartStats.min') }}
                        </th>
                        <th class="px-1.5 pb-0.5 text-right font-medium">
                            {{ t('components.chartStats.max') }}
                        </th>
                        <th class="pl-1.5 pb-0.5 text-right font-medium">
                            {{ t('components.chartStats.avg') }}
                        </th>
                        <template v-if="wide">
                            <th class="pl-6 pr-1.5 pb-0.5 text-right font-medium">
                                {{ t('views.alerts.above') }}
                            </th>
                            <th class="px-1.5 pb-0.5 text-right font-medium">
                                {{ t('views.alerts.below') }}
                            </th>
                            <th class="pl-1.5 pb-0.5 text-right font-medium">
                                {{ t('views.alerts.longest') }}
                            </th>
                        </template>
                    </tr>
                </thead>
                <tbody v-for="row in rows" :key="row.key">
                    <tr>
                        <!-- max-w-0 lets the name give way to the numbers and truncate. -->
                        <th class="w-full max-w-0 py-0.5 pr-1.5 text-left font-normal">
                            <span class="flex items-center gap-1.5">
                                <svg-icon
                                    type="mdi"
                                    :path="mdiMinusThick"
                                    :size="14"
                                    class="shrink-0"
                                    :style="{ color: row.source.lineColor }"
                                />
                                <span class="truncate">{{ row.source.channelFrontendName }}</span>
                            </span>
                        </th>
                        <td class="whitespace-nowrap px-1.5 py-0.5 text-right font-semibold">
                            {{ groupDigits(row.source.value) }}
                        </td>
                        <td class="px-1.5 py-0.5 text-right">
                            <AlertReferenceValue
                                v-if="row.stats != null"
                                :text="formatNumber(row.stats.min)"
                                :thresholds="thresholds"
                                @apply="relayApply"
                            />
                            <span v-else>-</span>
                        </td>
                        <td class="px-1.5 py-0.5 text-right">
                            <AlertReferenceValue
                                v-if="row.stats != null"
                                :text="formatNumber(row.stats.max)"
                                :thresholds="thresholds"
                                @apply="relayApply"
                            />
                            <span v-else>-</span>
                        </td>
                        <td class="whitespace-nowrap py-0.5 pl-1.5 text-right">
                            {{ row.stats != null ? formatNumber(row.stats.avg) : '-' }}
                        </td>
                        <template v-if="wide">
                            <td
                                class="whitespace-nowrap py-0.5 pl-6 pr-1.5 text-right"
                                :class="outsideClass(row.outside?.above ?? null)"
                            >
                                {{ row.outside?.above.text ?? '-' }}
                            </td>
                            <td
                                class="whitespace-nowrap px-1.5 py-0.5 text-right"
                                :class="outsideClass(row.outside?.below ?? null)"
                            >
                                {{ row.outside?.below?.text ?? '-' }}
                            </td>
                            <td
                                class="whitespace-nowrap py-0.5 pl-1.5 text-right"
                                :class="outsideClass(row.outside?.longest ?? null)"
                            >
                                {{ row.outside?.longest.text ?? '-' }}
                            </td>
                        </template>
                    </tr>
                    <tr v-if="row.limits.length > 0">
                        <td :colspan="columnCount" class="pb-1.5 pl-5">
                            <div class="flex flex-wrap items-center gap-x-3 gap-y-0.5">
                                <span class="text-xs text-text-color-secondary">
                                    {{ t('components.channelAttributes.title') }}
                                </span>
                                <AlertReferenceValue
                                    v-for="limit in row.limits"
                                    :key="limit.name"
                                    :hint="limit.name"
                                    :label="attributeLabel(limit.kind, t)"
                                    :text="formatAttributeNumber(limit)"
                                    :thresholds="thresholds"
                                    @apply="relayApply"
                                />
                            </div>
                        </td>
                    </tr>
                    <tr v-if="!wide && row.outside != null">
                        <td :colspan="columnCount" class="pb-1.5 pl-5">
                            <div class="flex flex-wrap items-center gap-x-3 gap-y-0.5">
                                <span class="text-xs text-text-color-secondary">
                                    {{
                                        t('components.chartStats.lastMinutes', {
                                            minutes: row.outside.minutes,
                                        })
                                    }}
                                </span>
                                <span
                                    v-for="item in outsideItems(row.outside)"
                                    :key="item.key"
                                    class="whitespace-nowrap"
                                    :class="outsideClass(item)"
                                >
                                    <span class="text-text-color-secondary"
                                        >{{ item.label }}&nbsp;</span
                                    >{{ item.text }}
                                </span>
                            </div>
                        </td>
                    </tr>
                </tbody>
                <tbody v-if="allSources != null">
                    <tr class="border-t border-border-one">
                        <th class="w-full max-w-0 truncate py-1 pr-1.5 text-left font-medium">
                            {{ t('views.alerts.allSources') }}
                        </th>
                        <td></td>
                        <td class="px-1.5 py-1 text-right">
                            <AlertReferenceValue
                                :text="formatNumber(allSources.min)"
                                :thresholds="thresholds"
                                @apply="relayApply"
                            />
                        </td>
                        <td class="px-1.5 py-1 text-right">
                            <AlertReferenceValue
                                :text="formatNumber(allSources.max)"
                                :thresholds="thresholds"
                                @apply="relayApply"
                            />
                        </td>
                        <td :colspan="columnCount - 4"></td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>
</template>
