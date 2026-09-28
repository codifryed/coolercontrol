// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Wire values of the daemon's ChannelAttributeKind (SCREAMING_SNAKE_CASE). Temperatures are in
// °C (hysteresis values are absolute temperatures), fan speeds in rpm, the rest plain numbers.
export type ChannelAttributeKind =
    | 'TEMP_MAX'
    | 'TEMP_MAX_HYST'
    | 'TEMP_CRIT'
    | 'TEMP_CRIT_HYST'
    | 'TEMP_EMERGENCY'
    | 'TEMP_EMERGENCY_HYST'
    | 'TEMP_MIN'
    | 'TEMP_MIN_HYST'
    | 'TEMP_LCRIT'
    | 'TEMP_LCRIT_HYST'
    | 'TEMP_LOWEST'
    | 'TEMP_HIGHEST'
    | 'TEMP_OFFSET'
    | 'TEMP_TYPE'
    | 'FAN_MIN'
    | 'FAN_MAX'
    | 'FAN_TARGET'
    | 'FAN_DIV'
    | 'FAN_PULSES'

// One driver-reported attribute. `name` is the sysfs file name, e.g. temp1_crit.
export interface ChannelAttribute {
    name: string
    kind: ChannelAttributeKind
    value: number
}

export interface ChannelAttributesResponse {
    attributes: Array<ChannelAttribute>
}
