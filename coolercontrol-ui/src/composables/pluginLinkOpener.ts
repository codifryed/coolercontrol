// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { escapeHtml, safeColor } from '@/components/htmlEscaping.ts'

/**
 * How the opener's link fits the button it lies over. The UI cannot style or observe
 * anything inside the frame, so hover and keyboard focus have to be drawn there.
 */
export interface OpenerLook {
    /** The UI's own color scheme. With any other the browser paints the frame opaque. */
    colorScheme: string
    /** The button's corner radius, as a computed style gives it. */
    radius: string
    /** A color that stands out against the button, for the keyboard focus ring. */
    ringColor: string
}

/**
 * How long a prompt is up before its opener takes a click. Longer than a double-click, so
 * the second click of one aimed at the plugin's page cannot land on the link unread.
 */
export const OPENER_ARM_DELAY_MS = 1000

const RADIUS_PATTERN = /^\d+(\.\d+)?px$/

/**
 * The document of the frame that opens a plugin link: one invisible link filling the frame,
 * to be laid over a button the UI draws.
 *
 * The link is opened from a sandboxed frame rather than by the UI, because a navigation the
 * UI starts is same-site and stays so through redirects. The session cookie would follow a
 * redirect back to this host on any port. A sandboxed frame has an opaque origin, so every
 * hop is cross-site and the cookie stays behind.
 */
export function openerDocument(url: URL, label: string, look: OpenerLook): string {
    const scheme =
        look.colorScheme === 'light' || look.colorScheme === 'dark' ? look.colorScheme : 'normal'
    const radius = RADIUS_PATTERN.test(look.radius) ? look.radius : '0'
    return (
        '<!doctype html><html><head><meta charset="utf-8"><style>' +
        `:root{color-scheme:${scheme}}` +
        'html,body{height:100%;margin:0;overflow:hidden;background:transparent}' +
        // The label is for screen readers only: the button underneath shows it.
        `a{display:block;height:100%;border-radius:${radius};font-size:0;outline:none}` +
        'a:hover{background:rgba(128,128,128,.2)}' +
        `a:focus-visible{box-shadow:inset 0 0 0 2px ${safeColor(look.ringColor)}}` +
        '</style></head><body>' +
        `<a href="${escapeHtml(url.href)}" target="_blank" rel="noopener noreferrer">` +
        `${escapeHtml(label)}</a></body></html>`
    )
}
