// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { ChannelNameOverrides } from '@/models/NameOverrides'

// hwmon has no flow, pressure or level type, so drivers report those through the rpm
// field and name the real unit in the label: "Flow [dL/h]". Same rule as the daemon's
// `label_unit`: a trailing bracket of at most 10 characters without spaces. Longer or
// spaced bracket text is a description, not a unit.
const TRAILING_UNIT = /\[([^\s[\]]{1,10})\]\s*$/u

// The unit a label names in its trailing brackets, as written.
export function labelUnit(label: string | undefined): string | undefined {
    if (label == null) return undefined
    return TRAILING_UNIT.exec(label)?.[1]
}

// The unit of a channel's rpm value, or undefined when it is rpm. The user's label is
// read first, then the detected one, and the first to name a unit wins, so a user's
// "[rpm]" resets a unit the driver named.
export function channelUnit(
    displayedLabel: string | undefined,
    overrides: ChannelNameOverrides | undefined,
): string | undefined {
    // An active override is the displayed label, and its hint is the detected one.
    const detectedLabel = overrides?.label != null ? overrides.channel_label : undefined
    for (const label of [displayedLabel, detectedLabel]) {
        const unit = labelUnit(label)
        if (unit != null) return unit.toLowerCase() === 'rpm' ? undefined : unit
    }
    return undefined
}
