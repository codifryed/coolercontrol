// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

const PLUGIN_LINK_LENGTH_MAX = 2048
const ALLOWED_PROTOCOLS = new Set(['http:', 'https:'])

const comparableHostname = (hostname: string): string => hostname.toLowerCase().replace(/\.$/, '')

/**
 * Validate a link supplied by a plugin before the UI opens it. Returns the parsed URL, or
 * null if it must not be opened. Open and show the returned URL, never the raw string.
 *
 * Only absolute http(s) URLs without credentials are taken. A link to one of `ownHostnames`
 * is refused on any port: opened from the UI it is a same-site navigation, so the session
 * cookie, which is not scoped to a port, would go with it.
 */
export function validatePluginLink(raw: unknown, ownHostnames: readonly string[]): URL | null {
    if (typeof raw !== 'string' || raw.length > PLUGIN_LINK_LENGTH_MAX) {
        return null
    }
    let url: URL
    try {
        url = new URL(raw)
    } catch {
        return null
    }
    if (!ALLOWED_PROTOCOLS.has(url.protocol)) {
        return null
    }
    if (url.username !== '' || url.password !== '') {
        return null
    }
    const hostname = comparableHostname(url.hostname)
    if (hostname === '' || ownHostnames.some((own) => comparableHostname(own) === hostname)) {
        return null
    }
    return url
}

/** The hostnames this UI and its daemon are reached at. */
export function uiHostnames(daemonURL: string): string[] {
    const hostnames = [window.location.hostname]
    try {
        hostnames.push(new URL(daemonURL).hostname)
    } catch {
        // An unparsable daemon address names no host to protect.
    }
    return hostnames.filter((hostname) => hostname !== '')
}
