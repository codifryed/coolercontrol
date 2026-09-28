<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiArrowTopRightBottomLeft, mdiClose, mdiRefresh } from '@mdi/js'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { DataType, getLocalizedDataType } from '@/models/Dashboard.ts'
import type { ChannelAttribute } from '@/models/ChannelAttributes.ts'
import type { ChannelStats } from '@/models/Stats.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useLifetimeStats } from '@/composables/useLifetimeStats.ts'
import {
    formatJitterValue,
    formatSpan,
    formatStatValue,
    lifetimeStatsOf,
    lifetimeToDisplay,
    statUnitSuffix,
    type WindowLineStats,
    type WindowStatsPayload,
} from '@/components/chartStats.ts'
import {
    attributeLabel,
    formatAttributeValue,
    limitColor,
    type ThresholdLine,
} from '@/components/channelAttributes.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import HelpIcon from '@/components/info/HelpIcon.vue'
import UiButton from '@/shell/ui/UiButton.vue'

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
const windowLabel = computed((): string =>
    props.payload?.zoomed
        ? t('components.statsPanel.visible', { duration: formatSpan(props.payload.spanSeconds) })
        : t('components.chartStats.lastMinutes', { minutes: props.rangeMinutes }),
)

const lifetimeOf = (line: WindowLineStats): ChannelStats | null =>
    lifetimeToDisplay(
        lifetimeStatsOf(lifetime.value, line.deviceUID, line.channelName, line.dataType),
        line.dataType,
        settingsStore.frequencyPrecision,
    )

const format = (value: number | null | undefined, dataType: DataType): string => {
    if (value == null) return '-'
    const precision = settingsStore.frequencyPrecision
    return formatStatValue(value, dataType, precision) + statUnitSuffix(dataType, precision, t)
}
const formatJitter = (value: number | null | undefined, dataType: DataType): string => {
    if (value == null) return '-'
    const precision = settingsStore.frequencyPrecision
    return formatJitterValue(value, dataType, precision) + statUnitSuffix(dataType, precision, t)
}

const statRows = [
    { key: 'min', label: 'components.chartStats.min' },
    { key: 'max', label: 'components.chartStats.max' },
    { key: 'avg', label: 'components.chartStats.avg' },
] as const

const movePanel = (): void => {
    settingsStore.sensorStatsPanelPosition =
        settingsStore.sensorStatsPanelPosition === 'top-left' ? 'bottom-right' : 'top-left'
}
</script>

<template>
    <div
        class="absolute z-10 max-h-[calc(100%-2rem)] w-72 max-w-[calc(100%-6rem)] overflow-y-auto rounded-lg border border-border-one bg-bg-two/90 text-sm shadow-lg"
        :class="positionClasses"
    >
        <div
            class="sticky top-0 z-10 flex items-center gap-1 border-b border-border-one bg-bg-two/95 py-1 pl-3 pr-1"
        >
            <span class="flex-1 font-semibold">{{ t('components.chartStats.title') }}</span>
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
                    <th class="px-3 pb-0.5 pt-1.5"></th>
                    <th class="px-3 pb-0.5 pt-1.5 text-right font-medium">{{ windowLabel }}</th>
                    <th class="px-3 pb-0.5 pt-1.5 text-right font-medium">
                        <span class="inline-flex items-center gap-1">
                            {{ t('components.statsPanel.sinceStart') }}
                            <HelpIcon
                                :text="t('components.statsPanel.sinceStartHelp')"
                                :size="0.9"
                            />
                        </span>
                    </th>
                </tr>
            </thead>
            <tbody v-for="line in lines" :key="line.lineName">
                <tr v-if="lines.length > 1">
                    <th
                        colspan="3"
                        class="px-3 pb-0.5 pt-2 text-left text-xs font-semibold uppercase tracking-wide text-text-color-secondary"
                    >
                        {{ getLocalizedDataType(line.dataType) }}
                    </th>
                </tr>
                <tr>
                    <th class="px-3 py-0.5 text-left font-normal text-text-color-secondary">
                        {{ t('components.chartStats.now') }}
                    </th>
                    <td colspan="2" class="px-3 py-0.5 text-center font-semibold">
                        {{ format(line.latest, line.dataType) }}
                    </td>
                </tr>
                <tr v-for="row in statRows" :key="row.key">
                    <th class="px-3 py-0.5 text-left font-normal text-text-color-secondary">
                        {{ t(row.label) }}
                    </th>
                    <td class="px-3 py-0.5 text-right">
                        {{ format(line.stats?.[row.key], line.dataType) }}
                    </td>
                    <td class="px-3 py-0.5 text-right">
                        {{ format(lifetimeOf(line)?.[row.key], line.dataType) }}
                    </td>
                </tr>
                <tr>
                    <th class="px-3 py-0.5 text-left font-normal text-text-color-secondary">
                        <span class="inline-flex items-center gap-1">
                            {{ t('components.statsPanel.jitter') }}
                            <HelpIcon :text="t('components.statsPanel.jitterHelp')" :size="0.9" />
                        </span>
                    </th>
                    <td class="px-3 py-0.5 text-right">
                        {{ formatJitter(line.stats?.jitter, line.dataType) }}
                    </td>
                    <td class="px-3 py-0.5 text-right text-text-color-secondary">-</td>
                </tr>
            </tbody>
        </table>
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
