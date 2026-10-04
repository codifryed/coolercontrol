// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { v4 as uuidV4 } from 'uuid'

const ID_ATTEMPTS = 10

const shortHash = (): string => uuidV4().slice(0, 8)

// A counter would hand a deleted sensor's id to the next one, which then inherits the
// dashboards, colors and alerts still pointing at that id.
export function newCustomSensorId(
    takenIds: ReadonlySet<string>,
    hash: () => string = shortHash,
): string {
    for (let attempt = 0; attempt < ID_ATTEMPTS; attempt++) {
        const id = `sensor_${hash()}`
        if (!takenIds.has(id)) return id
    }
    throw new Error('Could not generate a unique Custom Sensor ID')
}
