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
import { computed, onBeforeUnmount, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useLifetimeStats } from '@/composables/useLifetimeStats.ts'
import HelpIcon from '@/components/info/HelpIcon.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import AlertReferenceValue from '@/components/AlertReferenceValue.vue'
import type { ChannelAttribute } from '@/models/ChannelAttributes.ts'
import type { ChannelMetric } from '@/models/ChannelSource.ts'
import { formatStatValue } from '@/components/chartStats.ts'
import { groupDigits } from '@/shell/digitGroups.ts'
import { attributeLabel, formatAttributeNumber } from '@/components/channelAttributes.ts'
import {
    metricDataType,
    metricHasDriverLimits,
    observedRange,
    thresholdAttributes,
    type ReferenceSource,
    type ThresholdTarget,
    type Thresholds,
} from '@/components/alertReference.ts'

const props = defineProps<{
    sources: Array<ReferenceSource>
    metric: ChannelMetric | undefined
    thresholds: Thresholds
}>()
const emit = defineEmits<{ apply: [target: ThresholdTarget, value: number] }>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const { displayOf } = useLifetimeStats()

// Alert thresholds are in the daemon's own units, so no frequency precision applies.
const RAW_PRECISION = 1
const dataType = computed(() => (props.metric == null ? undefined : metricDataType(props.metric)))
const formatNumber = (value: number): string =>
    dataType.value == null ? String(value) : formatStatValue(value, dataType.value, RAW_PRECISION)

const channelKey = (source: ReferenceSource): string => `${source.deviceUID}/${source.channelName}`

// Driver attributes per channel. Each one costs a device read, so a channel is read once,
// when it is first selected, and only one at a time.
const attributes = shallowRef(new Map<string, Array<ChannelAttribute>>())
const hasDriverLimits = computed(() => metricHasDriverLimits(props.metric))
let reading = false
let unmounted = false
onBeforeUnmount(() => (unmounted = true))
// More reads than this in one pass means the selection keeps changing under it.
const READS_PER_PASS_MAX = 64
const readMissing = async (): Promise<void> => {
    if (reading) return
    reading = true
    try {
        for (let reads = 0; reads < READS_PER_PASS_MAX; reads++) {
            if (unmounted || !hasDriverLimits.value) return
            const next = props.sources.find((source) => !attributes.value.has(channelKey(source)))
            if (next == null) return
            const key = channelKey(next)
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
    () => props.sources.map(channelKey).join('|') + props.metric,
    () => void readMissing(),
    { immediate: true },
)

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
            key: channelKey(source),
            source,
            stats,
            limits: thresholdAttributes(
                attributes.value.get(channelKey(source)) ?? [],
                props.metric,
            ),
        }
    }),
)
// Only worth a row once there is more than one source to span.
const allSources = computed(() =>
    rows.value.length < 2 ? null : observedRange(rows.value.map((row) => row.stats)),
)
</script>

<template>
    <div class="px-4 py-3">
        <div class="flex items-center gap-1.5 pb-1">
            <span class="text-base text-text-color">{{ t('views.alerts.reference') }}</span>
            <HelpIcon :text="t('views.alerts.referenceHelp')" />
            <span class="flex-1"></span>
            <UiButton
                v-if="hasDriverLimits"
                variant="ghost"
                size="icon"
                class="!h-7 !w-7"
                :aria-label="t('components.channelAttributes.refresh')"
                v-tooltip.top="t('components.channelAttributes.refresh')"
                @click="readAgain"
            >
                <svg-icon type="mdi" :path="mdiRefresh" :size="deviceStore.getREMSize(1)" />
            </UiButton>
        </div>
        <div class="max-h-72 overflow-y-auto">
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
                                @apply="(target, value) => emit('apply', target, value)"
                            />
                            <span v-else>-</span>
                        </td>
                        <td class="px-1.5 py-0.5 text-right">
                            <AlertReferenceValue
                                v-if="row.stats != null"
                                :text="formatNumber(row.stats.max)"
                                :thresholds="thresholds"
                                @apply="(target, value) => emit('apply', target, value)"
                            />
                            <span v-else>-</span>
                        </td>
                        <td class="whitespace-nowrap py-0.5 pl-1.5 text-right">
                            {{ row.stats != null ? formatNumber(row.stats.avg) : '-' }}
                        </td>
                    </tr>
                    <tr v-if="row.limits.length > 0">
                        <td colspan="5" class="pb-1.5 pl-5">
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
                                    @apply="(target, value) => emit('apply', target, value)"
                                />
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
                                @apply="(target, value) => emit('apply', target, value)"
                            />
                        </td>
                        <td class="px-1.5 py-1 text-right">
                            <AlertReferenceValue
                                :text="formatNumber(allSources.max)"
                                :thresholds="thresholds"
                                @apply="(target, value) => emit('apply', target, value)"
                            />
                        </td>
                        <td></td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>
</template>
