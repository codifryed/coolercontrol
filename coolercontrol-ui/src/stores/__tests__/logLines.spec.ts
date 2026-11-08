// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import {
    appendLogChunk,
    highlightLogLine,
    localizeLogStamp,
    LOG_LINE_CAP,
    toLogLines,
} from '../logLines.ts'

// Pin the zone so local times do not depend on the machine. Berlin is +01:00
// in November and leaves DST at 2026-10-25T01:00:00Z.
const savedTz = process.env.TZ
beforeAll(() => {
    process.env.TZ = 'Europe/Berlin'
})
afterAll(() => {
    if (savedTz === undefined) delete process.env.TZ
    else process.env.TZ = savedTz
})

describe('localizeLogStamp', () => {
    it('converts a seconds stamp to local time', () => {
        expect(localizeLogStamp('[2026-11-08T17:21:40Z INFO  coolercontrold::logger] up')).toBe(
            '[2026-11-08 18:21:40 INFO  coolercontrold::logger] up',
        )
    })

    it('keeps milliseconds', () => {
        expect(localizeLogStamp('[2026-11-08T17:21:40.123Z DEBUG coolercontrold::x] m')).toBe(
            '[2026-11-08 18:21:40.123 DEBUG coolercontrold::x] m',
        )
    })

    it('rolls the date over at local midnight', () => {
        expect(localizeLogStamp('[2026-12-31T23:30:00Z WARN  x] m')).toBe(
            '[2027-01-01 00:30:00 WARN  x] m',
        )
    })

    it('follows the offset across a DST change', () => {
        expect(localizeLogStamp('[2026-10-25T00:59:59Z INFO  x] m')).toBe(
            '[2026-10-25 02:59:59 INFO  x] m',
        )
        expect(localizeLogStamp('[2026-10-25T01:00:00Z INFO  x] m')).toBe(
            '[2026-10-25 02:00:00 INFO  x] m',
        )
    })

    it('leaves continuation lines unchanged', () => {
        expect(localizeLogStamp('    at some::frame')).toBe('    at some::frame')
        expect(localizeLogStamp('')).toBe('')
    })

    it('changes only the leading stamp', () => {
        expect(localizeLogStamp('[2026-11-08T17:21:40Z INFO  x] since 2026-11-08T10:00:00Z')).toBe(
            '[2026-11-08 18:21:40 INFO  x] since 2026-11-08T10:00:00Z',
        )
        expect(localizeLogStamp('note [2026-11-08T17:21:40Z INFO  x]')).toBe(
            'note [2026-11-08T17:21:40Z INFO  x]',
        )
    })

    it('leaves invalid or non UTC stamps unchanged', () => {
        for (const line of [
            '[2026-13-08T17:21:40Z INFO  x] m',
            '[2026-02-30T17:21:40Z INFO  x] m',
            '[2026-11-08T25:21:40Z INFO  x] m',
            '[2026-11-08T17:21:40+01:00 INFO  x] m',
            '[2026-11-08T17:21:40Zoops INFO  x] m',
        ]) {
            expect(localizeLogStamp(line)).toBe(line)
        }
    })
})

describe('highlightLogLine', () => {
    it('escapes html and wraps log levels', () => {
        const html = highlightLogLine('WARN <bad> & done')
        expect(html).toBe('<span class="text-warning">WARN</span> &lt;bad&gt; &amp; done')
    })

    it('escapes before highlighting so injected spans stay inert', () => {
        const html = highlightLogLine('<span>ERROR</span>')
        expect(html).toBe('&lt;span&gt;<span class="text-error">ERROR</span>&lt;/span&gt;')
    })
})

describe('toLogLines', () => {
    it('splits multi-line events and drops empty lines', () => {
        const lines = toLogLines('one\ntwo\n\nthree\n')
        expect(lines.map((line) => line.raw)).toEqual(['one', 'two', 'three'])
        expect(lines[0].html).toBe('one')
    })

    it('localizes raw before highlighting so raw and html agree', () => {
        const [line] = toLogLines('[2026-11-08T17:21:40Z ERROR coolercontrold::x] bad\n')
        expect(line.raw).toBe('[2026-11-08 18:21:40 ERROR coolercontrold::x] bad')
        expect(line.raw.includes('ERROR')).toBe(true)
        expect(line.html).toBe(
            '[2026-11-08 18:21:40 <span class="text-error">ERROR</span> coolercontrold::x] bad',
        )
    })
})

describe('appendLogChunk', () => {
    it('appends chunks in order', () => {
        let lines = appendLogChunk([], 'a\nb\n')
        lines = appendLogChunk(lines, 'c\n')
        expect(lines.map((line) => line.raw)).toEqual(['a', 'b', 'c'])
    })

    it('drops the oldest lines past the cap', () => {
        const initial = appendLogChunk(
            [],
            Array.from({ length: LOG_LINE_CAP }, (_, i) => `line${i}`).join('\n'),
        )
        expect(initial).toHaveLength(LOG_LINE_CAP)
        const appended = appendLogChunk(initial, 'newest\n')
        expect(appended).toHaveLength(LOG_LINE_CAP)
        expect(appended[0].raw).toBe('line1')
        expect(appended[appended.length - 1].raw).toBe('newest')
    })
})
