// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A chart's series list (the label, unit, scale and colour of every line) is built once
// from its line names, while the data arrays are rebuilt whenever the status history is
// re-read. The two only line up while the names agree, so both the lookup and the
// comparison below exist to keep a value from landing on another line.

// Where a line's values live in the chart's data, or null when the chart has no such line.
// Index 0 is the timestamp row and line i follows at i + 1, so a missing line must not fall
// through to 0: that overwrites the timestamps of every series at once.
export function lineDataIndex(lineNames: readonly string[], lineName: string): number | null {
    const index = lineNames.indexOf(lineName)
    return index < 0 ? null : index + 1
}

// Whether a freshly read line set still matches the one the chart was built with. Order
// counts: the data arrays are addressed by position, so a reordering misplaces every value
// after the first difference.
export function lineSetMatches(built: readonly string[], current: readonly string[]): boolean {
    return built.length === current.length && built.every((name, i) => name === current[i])
}
