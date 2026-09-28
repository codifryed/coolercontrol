// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { ChannelAttribute, ChannelAttributeKind } from '@/models/ChannelAttributes.ts'
import { SCALE_KEY_PERCENT, SCALE_KEY_RPM } from '@/components/chartScales.ts'

type Translate = (key: string) => string

const KIND_LABEL_KEYS: Record<ChannelAttributeKind, string> = {
    TEMP_MAX: 'components.channelAttributes.kinds.tempMax',
    TEMP_MAX_HYST: 'components.channelAttributes.kinds.tempMaxHyst',
    TEMP_CRIT: 'components.channelAttributes.kinds.tempCrit',
    TEMP_CRIT_HYST: 'components.channelAttributes.kinds.tempCritHyst',
    TEMP_EMERGENCY: 'components.channelAttributes.kinds.tempEmergency',
    TEMP_EMERGENCY_HYST: 'components.channelAttributes.kinds.tempEmergencyHyst',
    TEMP_MIN: 'components.channelAttributes.kinds.tempMin',
    TEMP_MIN_HYST: 'components.channelAttributes.kinds.tempMinHyst',
    TEMP_LCRIT: 'components.channelAttributes.kinds.tempLcrit',
    TEMP_LCRIT_HYST: 'components.channelAttributes.kinds.tempLcritHyst',
    TEMP_LOWEST: 'components.channelAttributes.kinds.tempLowest',
    TEMP_HIGHEST: 'components.channelAttributes.kinds.tempHighest',
    TEMP_OFFSET: 'components.channelAttributes.kinds.tempOffset',
    TEMP_TYPE: 'components.channelAttributes.kinds.tempType',
    FAN_MIN: 'components.channelAttributes.kinds.fanMin',
    FAN_MAX: 'components.channelAttributes.kinds.fanMax',
    FAN_TARGET: 'components.channelAttributes.kinds.fanTarget',
    FAN_DIV: 'components.channelAttributes.kinds.fanDiv',
    FAN_PULSES: 'components.channelAttributes.kinds.fanPulses',
}

// hwmon ABI sensor type codes.
const TEMP_TYPE_KEYS: Record<number, string> = {
    1: 'components.channelAttributes.tempTypes.cpuDiode',
    2: 'components.channelAttributes.tempTypes.transistor',
    3: 'components.channelAttributes.tempTypes.thermalDiode',
    4: 'components.channelAttributes.tempTypes.thermistor',
    5: 'components.channelAttributes.tempTypes.amdAmdsi',
    6: 'components.channelAttributes.tempTypes.intelPeci',
}

export type LimitSeverity = 'warning' | 'critical' | 'fan'

// Which attributes are drawn as limit lines. Lower limits, hysteresis and sensor details are
// only listed: drawn, they crowd the chart without saying anything a reader acts on.
const LIMIT_SEVERITY: Partial<Record<ChannelAttributeKind, LimitSeverity>> = {
    TEMP_MAX: 'warning',
    TEMP_CRIT: 'critical',
    TEMP_EMERGENCY: 'critical',
    FAN_MIN: 'fan',
    FAN_MAX: 'fan',
}

export interface ThresholdLine {
    // The attribute's sysfs name, which ties the line to its row in the stats panel.
    name: string
    // In chart units: rpm is divided by the precision setting like the chart's rpm lines.
    value: number
    scale: string
    severity: LimitSeverity
    label: string
}

export interface LimitPalette {
    red: string
    yellow: string
    text_color_secondary: string
}

// One place for the line and swatch colours, so a panel row matches the line it names.
export const limitColor = (severity: LimitSeverity, palette: LimitPalette): string => {
    switch (severity) {
        case 'critical':
            return palette.red
        case 'warning':
            return palette.yellow
        default:
            return palette.text_color_secondary
    }
}

const isTemperature = (kind: ChannelAttributeKind): boolean =>
    kind.startsWith('TEMP_') && kind !== 'TEMP_TYPE'

export const attributeLabel = (kind: ChannelAttributeKind, t: Translate): string =>
    t(KIND_LABEL_KEYS[kind])

// Temperatures keep a second decimal when the driver reports one (nvme limits are x.85 °C).
export function formatAttributeNumber(attribute: ChannelAttribute): string {
    if (isTemperature(attribute.kind)) {
        const tenths = Math.round(attribute.value * 100) / 10
        return Number.isInteger(tenths) ? attribute.value.toFixed(1) : attribute.value.toFixed(2)
    }
    return attribute.value.toFixed(0)
}

export function formatAttributeValue(attribute: ChannelAttribute, t: Translate): string {
    const number = formatAttributeNumber(attribute)
    if (attribute.kind === 'TEMP_TYPE') {
        const key = TEMP_TYPE_KEYS[attribute.value]
        return key == null ? number : t(key)
    }
    if (isTemperature(attribute.kind)) return `${number} ${t('common.tempUnit')}`
    if (attribute.kind === 'FAN_DIV' || attribute.kind === 'FAN_PULSES') return number
    return `${number} ${t('common.rpmAbbr')}`
}

// The attributes worth drawing on the chart. A fan limit of 0 is left off: it sits on the axis,
// and on some chips it only means the fan never reported a speed.
export function thresholdLinesFrom(
    attributes: Array<ChannelAttribute>,
    precision: number,
    t: Translate,
): Array<ThresholdLine> {
    const lines: Array<ThresholdLine> = []
    for (const attribute of attributes) {
        const severity = LIMIT_SEVERITY[attribute.kind]
        if (severity == null) continue
        const label = `${attribute.name} ${formatAttributeValue(attribute, t)}`
        if (severity === 'fan') {
            if (attribute.value <= 0) continue
            lines.push({
                name: attribute.name,
                value: attribute.value / precision,
                scale: SCALE_KEY_RPM,
                severity,
                label,
            })
        } else {
            lines.push({
                name: attribute.name,
                value: attribute.value,
                scale: SCALE_KEY_PERCENT,
                severity,
                label,
            })
        }
    }
    return lines
}
