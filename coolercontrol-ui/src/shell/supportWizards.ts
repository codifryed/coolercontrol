// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A hidden thank-you to the volunteers who answer hardware and driver questions
// on Discord, which is the part of getting a fan working that no amount of code
// in here can do for anyone. Seven quick taps on the rail logo sets the wizards
// drifting across the screen, seven more sends them away. Session only, nothing
// persisted and nothing sent anywhere.
//
// They ride their own overlay rather than replacing any real glyph, so the egg
// stays one component and the rest of the app never learns it exists.

import { readonly, ref } from 'vue'

export const TAPS_REQUIRED = 3
// Taps only count as a run while they keep landing. Without the window the
// counter would collect the ordinary logo clicks of a long session and fire
// on its own, which would read as a bug rather than a secret.
const TAP_WINDOW_MS = 1500

const active = ref(false)
let taps = 0
let lastTapAt = 0

export const wizardsActive = readonly(active)

// True only on the tap that sends the wizards out, so the caller announces it
// once and stays quiet when they are dismissed.
export function registerLogoTap(now: number = Date.now()): boolean {
    taps = now - lastTapAt > TAP_WINDOW_MS ? 1 : taps + 1
    lastTapAt = now
    if (taps < TAPS_REQUIRED) return false
    taps = 0
    active.value = !active.value
    return active.value
}

/**
 * One wizard per line, so the roster is edited by editing this list.
 *
 * Deliberately hardcoded English and deliberately absent from `i18n/locales`.
 * These are in-jokes about a support channel that runs in English; translating
 * them would cost eleven translators real effort to produce eleven jokes that
 * do not land. The i18n key sweep only follows translation-function calls, so
 * plain strings here stay invisible to it.
 */
export const SAYINGS: readonly string[] = [
    'Run sensors-detect and paste the output',
    'That is a driver limitation, not CoolerControl',
    'Have you checked the BIOS fan settings?',
    'Please post your hardware report',
    'Which kernel are you on?',
    'That header is 3-pin, not PWM',
    'Your hardware is broken, not the app',
    'Is the daemon actually running?',
]

// The bouncing box is the whole card, hat plus label, so a wizard turns before
// its own words clip the edge. Fixed rather than measured: the label wraps
// inside this width, which keeps the geometry pure and the math testable.
export const SPRITE_W = 176
export const SPRITE_H = 76
const SPEED_PX_PER_SEC = 70
// A backgrounded tab hands back one enormous frame gap on return. Capping the
// step keeps that from teleporting a wizard through a wall instead of off it.
const MAX_STEP_SEC = 0.05
const HUE_PER_BOUNCE = 47

export interface Drift {
    x: number
    y: number
    vx: number
    vy: number
    hue: number
}

const clamp = (value: number, max: number): number => Math.min(Math.max(value, 0), max)

export function initialDrift(width: number, height: number, random = Math.random): Drift {
    const maxX = Math.max(0, width - SPRITE_W)
    const maxY = Math.max(0, height - SPRITE_H)
    // Held between 22.5 and 67.5 degrees so a wizard crosses the screen rather
    // than creeping along one edge. The speed jitter and the starting hue are
    // what keep a roster of them from moving and looking as one block.
    const angle = (0.25 + random() * 0.5) * (Math.PI / 2)
    const speed = SPEED_PX_PER_SEC * (0.85 + random() * 0.3)
    return {
        x: random() * maxX,
        y: random() * maxY,
        vx: speed * Math.cos(angle) * (random() < 0.5 ? -1 : 1),
        vy: speed * Math.sin(angle) * (random() < 0.5 ? -1 : 1),
        hue: Math.floor(random() * 360),
    }
}

// The DVD-logo bounce. Bounds come from the caller every frame, so a resize
// walks a wizard back inside without a listener, and the hue shifts on each
// wall to keep the corner hit worth waiting for.
export function advanceDrift(drift: Drift, seconds: number, width: number, height: number): Drift {
    const maxX = Math.max(0, width - SPRITE_W)
    const maxY = Math.max(0, height - SPRITE_H)
    const step = Math.min(Math.max(seconds, 0), MAX_STEP_SEC)
    let { vx, vy, hue } = drift
    let x = drift.x + vx * step
    let y = drift.y + vy * step
    let bounced = false
    if (x <= 0 || x >= maxX) {
        vx = -vx
        x = clamp(x, maxX)
        bounced = true
    }
    if (y <= 0 || y >= maxY) {
        vy = -vy
        y = clamp(y, maxY)
        bounced = true
    }
    if (bounced) hue = (hue + HUE_PER_BOUNCE) % 360
    return { x, y, vx, vy, hue }
}
