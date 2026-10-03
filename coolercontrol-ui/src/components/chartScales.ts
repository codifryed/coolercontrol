// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The time chart's uPlot scale keys. Kept apart from the plugins so pure helpers can name a
// scale without importing uPlot, which needs a browser to load.
export const SCALE_KEY_PERCENT = '%' as const
export const SCALE_KEY_RPM = 'rpm' as const
export const SCALE_KEY_WATTS = 'W' as const

export type ScaleKey = typeof SCALE_KEY_PERCENT | typeof SCALE_KEY_RPM | typeof SCALE_KEY_WATTS

const STEP_MULTIPLIERS = [1, 2, 2.5, 5]
// More decades than any reading the daemon can send spans.
const STEP_DECADES_MAX = 12

// Tick steps from the base up past the top of the scale. uPlot takes the first step whose
// ticks have room and draws no ticks when none has, so the list has to reach the scale:
// a fan input can carry a pressure in the hundreds of thousands.
export const tickSteps = (base: number, scaleMax: number): number[] => {
    const steps: number[] = []
    let unit = base
    for (let decade = 0; decade < STEP_DECADES_MAX; decade++) {
        for (const multiplier of STEP_MULTIPLIERS) steps.push(unit * multiplier)
        unit *= 10
        if (unit > scaleMax) break
    }
    steps.push(unit)
    return steps
}
