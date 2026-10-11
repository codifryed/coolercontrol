// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { useI18n } from 'vue-i18n'
import { DataType } from '@/models/Dashboard.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { groupDigits } from '@/shell/digitGroups.ts'
import {
    formatJitterValue,
    formatSpan,
    formatStatValue,
    statUnitSuffix,
    type LineKey,
    type WindowStatsPayload,
} from '@/components/chartStats.ts'

export interface StatFormat {
    unitSuffix: (line: LineKey) => string
    // A stat with its unit, or '-' when there is none.
    formatStat: (value: number | null | undefined, line: LineKey) => string
    formatJitter: (value: number | null | undefined, line: LineKey) => string
    // What the window stats cover: the time range, or the visible span when zoomed.
    windowLabelOf: (payload: WindowStatsPayload | null, rangeMinutes: number) => string
}

// Stat text for the time chart's legend and stats panel, in the user's frequency precision.
export function useStatFormat(): StatFormat {
    const settingsStore = useSettingsStore()
    const { t } = useI18n()

    // An rpm line takes the unit its channel's label names.
    const unitSuffix = (line: LineKey): string =>
        line.dataType === DataType.RPM
            ? ` ${settingsStore.rpmUnit(line.deviceUID, line.channelName)}`
            : statUnitSuffix(line.dataType, settingsStore.frequencyPrecision, t)
    const formatStat = (value: number | null | undefined, line: LineKey): string =>
        value == null
            ? '-'
            : groupDigits(formatStatValue(value, line.dataType, settingsStore.frequencyPrecision)) +
              unitSuffix(line)
    const formatJitter = (value: number | null | undefined, line: LineKey): string =>
        value == null
            ? '-'
            : groupDigits(
                  formatJitterValue(value, line.dataType, settingsStore.frequencyPrecision),
              ) + unitSuffix(line)
    const windowLabelOf = (payload: WindowStatsPayload | null, rangeMinutes: number): string =>
        payload?.zoomed
            ? t('components.chartStats.visibleRange', { duration: formatSpan(payload.spanSeconds) })
            : t('components.chartStats.lastMinutes', { minutes: rangeMinutes })

    return { unitSuffix, formatStat, formatJitter, windowLabelOf }
}
