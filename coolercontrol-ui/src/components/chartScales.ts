// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The time chart's uPlot scale keys. Kept apart from the plugins so pure helpers can name a
// scale without importing uPlot, which needs a browser to load.
export const SCALE_KEY_PERCENT = '%' as const
export const SCALE_KEY_RPM = 'rpm' as const
export const SCALE_KEY_WATTS = 'W' as const

export type ScaleKey = typeof SCALE_KEY_PERCENT | typeof SCALE_KEY_RPM | typeof SCALE_KEY_WATTS
