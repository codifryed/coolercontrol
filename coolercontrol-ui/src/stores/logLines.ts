// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Log lines are escaped and highlighted once on arrival; views render the
// pre-built html per line. The cap mirrors the daemon's own log buffer so the
// UI state always looks like a fresh /logs fetch.

export const LOG_LINE_CAP = 500

export interface LogLine {
    raw: string
    html: string
}

const LEVEL_SPANS: Array<[string, string]> = [
    ['INFO', '<span class="text-success">INFO</span>'],
    ['ERROR', '<span class="text-error">ERROR</span>'],
    ['WARN', '<span class="text-warning">WARN</span>'],
    ['DEBUG', '<span class="text-info">DEBUG</span>'],
    ['TRACE', '<span class="text-pink">TRACE</span>'],
]

export function highlightLogLine(raw: string): string {
    let html = raw.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
    for (const [level, span] of LEVEL_SPANS) {
        html = html.replaceAll(level, span)
    }
    return html
}

// The daemon stamps lines in UTC: `[2026-11-08T17:21:40Z` or `[...40.123Z`.
const UTC_STAMP = /^\[(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?Z(?=[ \]])/
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

const pad2 = (value: number): string => String(value).padStart(2, '0')

// Rewrites a leading UTC stamp to journalctl's local `Nov 08 18:21:40` form,
// so lines match the journal. Other lines are returned unchanged.
export function localizeLogStamp(raw: string): string {
    const match = UTC_STAMP.exec(raw)
    if (match === null) return raw
    const [stamp, year, month, day, hour, minute, second, fraction = ''] = match
    const date = new Date(Date.UTC(+year, +month - 1, +day, +hour, +minute, +second))
    // Date.UTC rolls over out of range parts (month 13); reject those.
    const roundTrip = date.toISOString().slice(0, 19)
    if (roundTrip !== `${year}-${month}-${day}T${hour}:${minute}:${second}`) return raw
    // Offsets are whole minutes, so the fraction carries over verbatim.
    const local =
        `${MONTHS[date.getMonth()]} ${pad2(date.getDate())} ` +
        `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`
    return `[${local}${fraction}${raw.slice(stamp.length)}`
}

export function toLogLines(chunk: string): LogLine[] {
    return chunk
        .split('\n')
        .filter((line) => line.length > 0)
        .map((line) => {
            const raw = localizeLogStamp(line)
            return { raw, html: highlightLogLine(raw) }
        })
}

// Appends a (possibly multi-line) chunk, dropping the oldest lines past the cap.
export function appendLogChunk(lines: LogLine[], chunk: string): LogLine[] {
    const appended = [...lines, ...toLogLines(chunk)]
    return appended.length > LOG_LINE_CAP
        ? appended.slice(appended.length - LOG_LINE_CAP)
        : appended
}
