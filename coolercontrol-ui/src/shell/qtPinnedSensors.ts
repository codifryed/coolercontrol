// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The tray shows the sensors the user pinned, so a glance at the menu answers "what are
// my temps doing" without reopening the window. Qt cannot ask the SPA for the values:
// once the window is in the tray the renderer is discarded. So the SPA pushes only the
// identity and label of each pin, and Qt fetches the readings itself.
//
// Only sensor pins are pushed. `pinnedIds` also holds dashboard UIDs, which are
// distinguished here by matching against the known sensor set rather than by guessing
// at the id format.
//
// Colour and route are resolved here rather than in Qt: the colour falls back through
// the same accessor the Monitoring panel uses, and channelRoute deliberately sends a
// controllable fan to its Cooling page and a custom sensor to its editor. Duplicating
// either rule in C++ would let the tray drift away from the rest of the UI. The unit of a
// speed value is read from the label here for the same reason.

import type { Device, UID } from '@/models/Device'
import type { ChannelUnitOf } from '@/shell/channelUnit.ts'
import { monitoringSensors } from '@/shell/monitoring/sensors.ts'
import { pinId } from '@/shell/cooling/channels.ts'

// No cap: the user pinned these deliberately, and the tray moves them into a submenu
// once there are enough to crowd the main menu. Readings cost one bulk request per poll
// regardless of how many are pinned, so the list length does not drive request volume.

export interface QtPinnedSensor {
    deviceUid: UID
    channelName: string
    label: string
    isTemp: boolean
    /** Effective colour (user's if set, else the generated default), as the panel shows it. */
    color: string
    /** Resolved canonical target, so Qt never has to reimplement channelRoute. */
    route: string
    /** Unit of the speed value when the label names one. Absent means rpm. */
    unit?: string
}

export function buildPinnedSensors(
    devices: Iterable<Device>,
    pinnedIds: string[],
    label: (deviceUid: UID, channelName: string) => string,
    color: (deviceUid: UID, channelName: string) => string,
    route: (deviceUid: UID, channelName: string) => string,
    unit: ChannelUnitOf,
): QtPinnedSensor[] {
    const known = new Map<string, { deviceUID: UID; channelName: string; isTemp: boolean }>()
    for (const group of monitoringSensors(devices)) {
        for (const sensor of group.sensors) {
            known.set(pinId(sensor.deviceUID, sensor.channelName), sensor)
        }
    }
    // pinnedIds order is the user's chosen order, so preserve it.
    return pinnedIds
        .map((id) => known.get(id))
        .filter((s): s is NonNullable<typeof s> => s != null)
        .map((s) => ({
            deviceUid: s.deviceUID,
            channelName: s.channelName,
            label: label(s.deviceUID, s.channelName),
            isTemp: s.isTemp,
            color: color(s.deviceUID, s.channelName),
            route: route(s.deviceUID, s.channelName),
            unit: s.isTemp ? undefined : unit(s.deviceUID, s.channelName),
        }))
}
