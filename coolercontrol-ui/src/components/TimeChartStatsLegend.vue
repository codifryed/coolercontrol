<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { UID } from '@/models/Device.ts'
import { DataType } from '@/models/Dashboard.ts'
import type { StatsLegendScope } from '@/models/UISettings.ts'
import type { ChannelStats } from '@/models/Stats.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useLifetimeStats } from '@/composables/useLifetimeStats.ts'
import {
    formatSpan,
    formatStatValue,
    lifetimeStatsOf,
    lifetimeToDisplay,
    lineDash,
    statUnitSuffix,
    type WindowLineStats,
    type WindowStats,
    type WindowStatsPayload,
} from '@/components/chartStats.ts'
import HelpIcon from '@/components/info/HelpIcon.vue'
import UiToggleGroup, { type UiToggleOption } from '@/shell/ui/UiToggleGroup.vue'

interface Props {
    payload: WindowStatsPayload | null
    rangeMinutes: number
}

const props = defineProps<Props>()
const emit = defineEmits<{ (e: 'focusLine', seriesIndex: number | null): void }>()
const settingsStore = useSettingsStore()
const { t } = useI18n()

const windowLabel = computed((): string =>
    props.payload?.zoomed
        ? t('components.chartStats.visibleRange', {
              duration: formatSpan(props.payload.spanSeconds),
          })
        : t('components.chartStats.lastMinutes', { minutes: props.rangeMinutes }),
)

const { stats: lifetime } = useLifetimeStats()
const scope = computed<string>({
    get: () => settingsStore.statsLegendScope,
    set: (value) => (settingsStore.statsLegendScope = value as StatsLegendScope),
})
const scopeOptions = computed((): Array<UiToggleOption> => [
    { label: windowLabel.value, value: 'window' },
    { label: t('components.chartStats.sinceStart'), value: 'since-start' },
])

interface LegendRow {
    line: WindowLineStats
    stats: WindowStats | ChannelStats | null
}
const rows = computed((): Array<LegendRow> =>
    (props.payload?.lines ?? []).map((line) => ({
        line,
        stats:
            settingsStore.statsLegendScope === 'since-start'
                ? lifetimeToDisplay(
                      lifetimeStatsOf(
                          lifetime.value,
                          line.deviceUID,
                          line.channelName,
                          line.dataType,
                      ),
                      line.dataType,
                      settingsStore.frequencyPrecision,
                  )
                : line.stats,
    })),
)

const deviceName = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.name ?? ''

const format = (value: number | null | undefined, dataType: DataType): string => {
    if (value == null) return '-'
    const precision = settingsStore.frequencyPrecision
    return formatStatValue(value, dataType, precision) + statUnitSuffix(dataType, precision, t)
}
</script>

<template>
    <div id="time-chart-legend" class="border-t border-border-one">
        <div class="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 px-4 pb-1 pt-2">
            <span class="font-semibold">{{ t('components.chartStats.title') }}</span>
            <span class="inline-flex items-center gap-1.5">
                <UiToggleGroup
                    v-model="scope"
                    size="sm"
                    :aria-label="t('components.chartStats.scope')"
                    :options="scopeOptions"
                />
                <HelpIcon :text="t('components.chartStats.sinceStartHelp')" :size="0.9" />
            </span>
        </div>
        <div class="max-h-[35vh] overflow-auto px-2 pb-2">
            <table class="w-full min-w-[32rem] border-collapse text-sm tabular-nums">
                <thead>
                    <tr class="text-text-color-secondary">
                        <th class="legend-head text-left">
                            {{ t('components.chartStats.series') }}
                        </th>
                        <th class="legend-head">{{ t('components.chartStats.now') }}</th>
                        <th class="legend-head">{{ t('components.chartStats.min') }}</th>
                        <th class="legend-head">{{ t('components.chartStats.max') }}</th>
                        <th class="legend-head">{{ t('components.chartStats.avg') }}</th>
                    </tr>
                </thead>
                <tbody @mouseleave="emit('focusLine', null)">
                    <tr
                        v-for="{ line, stats } in rows"
                        :key="line.lineName"
                        class="hover:bg-surface-hover"
                        @mouseenter="emit('focusLine', line.seriesIndex)"
                    >
                        <td class="legend-cell text-left">
                            <div class="flex min-w-0 items-center gap-2">
                                <svg width="22" height="8" class="shrink-0" aria-hidden="true">
                                    <line
                                        x1="1"
                                        y1="4"
                                        x2="21"
                                        y2="4"
                                        :stroke="line.color"
                                        stroke-width="2"
                                        :stroke-dasharray="lineDash(line.lineName).join(' ')"
                                    />
                                </svg>
                                <span class="truncate">{{ line.label }}</span>
                                <span class="truncate text-xs text-text-color-secondary">{{
                                    deviceName(line.deviceUID)
                                }}</span>
                            </div>
                        </td>
                        <td class="legend-cell font-semibold">
                            {{ format(line.latest, line.dataType) }}
                        </td>
                        <td class="legend-cell">{{ format(stats?.min, line.dataType) }}</td>
                        <td class="legend-cell">{{ format(stats?.max, line.dataType) }}</td>
                        <td class="legend-cell">{{ format(stats?.avg, line.dataType) }}</td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>
</template>

<style scoped>
.legend-head {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 0.375rem 0.5rem;
    text-align: right;
    font-weight: 500;
    white-space: nowrap;
    background-color: rgb(var(--colors-bg-one));
    border-bottom: 1px solid rgb(var(--colors-border-one));
}
.legend-head.text-left {
    text-align: left;
}
.legend-cell {
    padding: 0.25rem 0.5rem;
    text-align: right;
    white-space: nowrap;
    border-bottom: 1px solid rgb(var(--colors-border-one) / 0.5);
}
.legend-cell.text-left {
    text-align: left;
    white-space: normal;
}
</style>
