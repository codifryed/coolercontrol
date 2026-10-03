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
    type WindowStatsPayload,
} from '@/components/chartStats.ts'

export interface StatFormat {
    unitSuffix: (dataType: DataType) => string
    // A stat with its unit, or '-' when there is none.
    formatStat: (value: number | null | undefined, dataType: DataType) => string
    formatJitter: (value: number | null | undefined, dataType: DataType) => string
    // What the window stats cover: the time range, or the visible span when zoomed.
    windowLabelOf: (payload: WindowStatsPayload | null, rangeMinutes: number) => string
}

// Stat text for the time chart's legend and stats panel, in the user's frequency precision.
export function useStatFormat(): StatFormat {
    const settingsStore = useSettingsStore()
    const { t } = useI18n()

    const unitSuffix = (dataType: DataType): string =>
        statUnitSuffix(dataType, settingsStore.frequencyPrecision, t)
    const formatStat = (value: number | null | undefined, dataType: DataType): string =>
        value == null
            ? '-'
            : groupDigits(formatStatValue(value, dataType, settingsStore.frequencyPrecision)) +
              unitSuffix(dataType)
    const formatJitter = (value: number | null | undefined, dataType: DataType): string =>
        value == null
            ? '-'
            : groupDigits(formatJitterValue(value, dataType, settingsStore.frequencyPrecision)) +
              unitSuffix(dataType)
    const windowLabelOf = (payload: WindowStatsPayload | null, rangeMinutes: number): string =>
        payload?.zoomed
            ? t('components.chartStats.visibleRange', { duration: formatSpan(payload.spanSeconds) })
            : t('components.chartStats.lastMinutes', { minutes: rangeMinutes })

    return { unitSuffix, formatStat, formatJitter, windowLabelOf }
}
