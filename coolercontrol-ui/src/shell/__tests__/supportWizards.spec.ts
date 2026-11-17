// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Two things carry this easter egg: the tap run, which sits on a link users
// click for ordinary reasons and so needs the window to stay a secret rather
// than a glitch, and the bounce, which has to stay inside the viewport through
// resizes and dropped frames. Each test takes a fresh module because the tap
// counter is module state by design (one rail, one run).

import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
    advanceDrift,
    initialDrift,
    SAYINGS,
    SPRITE_H,
    SPRITE_W,
    type Drift,
} from '@/shell/supportWizards.ts'

const load = async () => {
    vi.resetModules()
    return import('@/shell/supportWizards.ts')
}

const START = 1_000_000
const W = 800
const H = 600

describe('support wizard taps', () => {
    beforeEach(() => vi.resetModules())

    it('stays off until the run completes', async () => {
        const wiz = await load()
        for (let i = 1; i < wiz.TAPS_REQUIRED; i++) {
            expect(wiz.registerLogoTap(START + i * 100)).toBe(false)
            expect(wiz.wizardsActive.value).toBe(false)
        }
        expect(wiz.registerLogoTap(START + wiz.TAPS_REQUIRED * 100)).toBe(true)
        expect(wiz.wizardsActive.value).toBe(true)
    })

    it('drops a run when a tap lands outside the window', async () => {
        const wiz = await load()
        const last = wiz.TAPS_REQUIRED - 1
        for (let i = 1; i <= last; i++) wiz.registerLogoTap(START + i * 100)
        // Restarts the count at one, so this completes the run by wall clock
        // but is only the first tap of its own.
        expect(wiz.registerLogoTap(START + last * 100 + 1501)).toBe(false)
        expect(wiz.wizardsActive.value).toBe(false)
    })

    it('turns back off on a second run, silently', async () => {
        const wiz = await load()
        const run = wiz.TAPS_REQUIRED * 100
        for (let i = 1; i <= wiz.TAPS_REQUIRED; i++) wiz.registerLogoTap(START + i * 100)
        expect(wiz.wizardsActive.value).toBe(true)
        for (let i = 1; i < wiz.TAPS_REQUIRED; i++) wiz.registerLogoTap(START + run + i * 100)
        expect(wiz.registerLogoTap(START + 2 * run)).toBe(false)
        expect(wiz.wizardsActive.value).toBe(false)
    })
})

describe('support wizard drift', () => {
    it('starts in bounds and moving on both axes', () => {
        const drift = initialDrift(W, H, () => 0.5)
        expect(drift.x).toBeGreaterThanOrEqual(0)
        expect(drift.x).toBeLessThanOrEqual(W - SPRITE_W)
        expect(drift.y).toBeGreaterThanOrEqual(0)
        expect(drift.y).toBeLessThanOrEqual(H - SPRITE_H)
        expect(Math.abs(drift.vx)).toBeGreaterThan(0)
        expect(Math.abs(drift.vy)).toBeGreaterThan(0)
    })

    it('reflects off a wall and shifts hue', () => {
        const at: Drift = { x: 4, y: 300, vx: -100, vy: 0, hue: 0 }
        const next = advanceDrift(at, 0.05, W, H)
        expect(next.x).toBe(0)
        expect(next.vx).toBe(100)
        expect(next.hue).toBeGreaterThan(0)
    })

    it('leaves hue alone mid-flight', () => {
        const at: Drift = { x: 400, y: 300, vx: 70, vy: 70, hue: 94 }
        expect(advanceDrift(at, 0.02, W, H).hue).toBe(94)
    })

    it('stays in bounds across a long dropped frame', () => {
        // A backgrounded tab returns one huge gap; uncapped it would put the
        // hat thousands of pixels off screen with the velocity still outbound.
        const at: Drift = { x: 400, y: 300, vx: 70, vy: 70, hue: 0 }
        const next = advanceDrift(at, 30, W, H)
        expect(next.x).toBeGreaterThanOrEqual(0)
        expect(next.x).toBeLessThanOrEqual(W - SPRITE_W)
        expect(next.y).toBeGreaterThanOrEqual(0)
        expect(next.y).toBeLessThanOrEqual(H - SPRITE_H)
    })

    it('walks back inside when the window shrinks under it', () => {
        const at: Drift = { x: 700, y: 500, vx: 70, vy: 70, hue: 0 }
        const next = advanceDrift(at, 0.02, 600, 400)
        expect(next.x).toBe(600 - SPRITE_W)
        expect(next.y).toBe(400 - SPRITE_H)
        expect(next.vx).toBe(-70)
        expect(next.vy).toBe(-70)
    })

    it('survives a viewport smaller than one card', () => {
        const next = advanceDrift({ x: 5, y: 5, vx: 70, vy: 70, hue: 0 }, 0.02, 120, 40)
        expect(next.x).toBe(0)
        expect(next.y).toBe(0)
    })
})

describe('support wizard roster', () => {
    // The overlay keys its cards by saying, so a duplicate would drop a wizard
    // rather than render two, and an empty roster would make the whole run of
    // taps do nothing visible.
    it('has a wizard for every saying, each distinct', () => {
        expect(SAYINGS.length).toBeGreaterThan(1)
        expect(new Set(SAYINGS).size).toBe(SAYINGS.length)
    })

    // These are in-jokes for an English-speaking channel and are deliberately
    // outside the locale files; a well-meaning sweep pulling them in would put
    // eleven translators to work on eleven jokes that do not land.
    it('keeps the sayings out of the locale files', async () => {
        const locales = import.meta.glob('../../i18n/locales/*.ts', {
            query: '?raw',
            import: 'default',
            eager: true,
        }) as Record<string, string>
        expect(Object.keys(locales).length).toBeGreaterThan(5)
        const leaked: string[] = []
        for (const [path, source] of Object.entries(locales)) {
            for (const saying of SAYINGS) {
                if (source.includes(saying)) leaked.push(`${path}: ${saying}`)
            }
        }
        expect(leaked).toEqual([])
    })
})
