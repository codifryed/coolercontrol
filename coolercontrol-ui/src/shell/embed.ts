// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

/**
 * Messages exchanged with a page that shows the UI in a frame, such as a server
 * dashboard. Their names and shapes are a public interface that integrations are
 * written against: a change here has to leave an existing one working.
 */

import { type SystemPalette, systemPaletteFrom } from '@/shell/themes.ts'

/** True inside another page's frame. False in a browser tab and in the Qt app. */
const isEmbedded = (): boolean => window.parent !== window

/** The moments of startup an embedding page is told about. */
export type EmbedNotice = 'load-start' | 'palette-request' | 'load-end'

/**
 * Tells the embedding page how far startup has come. Sent to any origin: the
 * message is a constant, and only a page the daemon allows can be the parent.
 * Does nothing outside a frame.
 */
export const notifyEmbedder = (notice: EmbedNotice): void => {
    if (isEmbedded()) window.parent.postMessage({ type: `coolercontrol:${notice}` }, '*')
}

/**
 * Whether a message is the embedding page's. Any window can post here, the UI's
 * own plugin frames included, so the sender has to be the page directly above,
 * on this scheme and hostname. Its port is free to differ.
 */
const isFromEmbedder = (event: MessageEvent<unknown>): boolean => {
    if (event.source !== window.parent) return false
    try {
        const sender = new URL(event.origin)
        return (
            sender.protocol === window.location.protocol &&
            sender.hostname === window.location.hostname
        )
    } catch {
        // A sandboxed page's origin is the string "null", which is not a URL.
        return false
    }
}

/**
 * Asks the embedding page for its palette and hands over every one it sends. The
 * page may send again at any time, to follow its own theme. Does nothing outside
 * a frame.
 */
export const requestEmbedderPalette = (apply: (palette: SystemPalette) => void): void => {
    if (!isEmbedded()) return
    window.addEventListener('message', (event: MessageEvent<unknown>) => {
        if (!isFromEmbedder(event)) return
        if (event.data == null || typeof event.data !== 'object') return
        const message = event.data as Record<string, unknown>
        if (message.type !== 'coolercontrol:palette') return
        const palette = systemPaletteFrom(message.palette)
        if (palette == null) {
            console.error('failed to parse palette:', message.palette)
            return
        }
        apply(palette)
    })
    notifyEmbedder('palette-request')
}
