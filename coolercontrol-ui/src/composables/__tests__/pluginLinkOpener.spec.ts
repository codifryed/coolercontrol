// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, it, expect } from 'vitest'
import { openerDocument, type OpenerLook } from '../pluginLinkOpener'

const LOOK: OpenerLook = { colorScheme: 'dark', radius: '8px', ringColor: 'rgb(255, 255, 255)' }

// What the opener frame's document parses to, as a browser would see it.
function parse(html: string): Document {
    return new DOMParser().parseFromString(html, 'text/html')
}

function styleOf(look: Partial<OpenerLook>): string {
    const html = openerDocument(new URL('https://example.com/'), 'Open', { ...LOOK, ...look })
    return parse(html).querySelector('style')!.textContent!
}

describe('openerDocument', () => {
    it('holds one link to the URL that opens a new tab without an opener', () => {
        const doc = parse(openerDocument(new URL('https://example.com/a?b=1&c=2'), 'Open', LOOK))
        const anchors = doc.querySelectorAll('a')

        expect(anchors).toHaveLength(1)
        expect(anchors[0].getAttribute('href')).toBe('https://example.com/a?b=1&c=2')
        expect(anchors[0].target).toBe('_blank')
        expect(anchors[0].rel).toBe('noopener noreferrer')
        expect(anchors[0].textContent).toBe('Open')
    })

    it('keeps text that reads as an HTML entity in the URL as written', () => {
        const url = new URL('https://example.com/?a=1&quot;&lt;b&amp;c')
        const doc = parse(openerDocument(url, 'Open', LOOK))

        expect(doc.querySelector('a')!.getAttribute('href')).toBe(url.href)
        expect(url.href).toContain('&quot;&lt;b&amp;c')
    })

    it('keeps a hostile URL inside the href', () => {
        const url = new URL(`https://example.com/"><script>alert(1)</script>?q='"><img src=x>`)
        const doc = parse(openerDocument(url, 'Open', LOOK))

        expect(doc.querySelectorAll('a')).toHaveLength(1)
        expect(doc.querySelector('a')!.getAttribute('href')).toBe(url.href)
        expect(doc.querySelector('script')).toBeNull()
        expect(doc.querySelector('img')).toBeNull()
    })

    it('keeps a hostile label as text', () => {
        const label = '</a><script>alert(1)</script>'
        const doc = parse(openerDocument(new URL('https://example.com/'), label, LOOK))

        expect(doc.querySelector('script')).toBeNull()
        expect(doc.querySelector('a')!.textContent).toBe(label)
    })

    it.each(['light', 'dark'])('declares the %s color scheme of the UI', (colorScheme) => {
        expect(styleOf({ colorScheme })).toContain(`:root{color-scheme:${colorScheme}}`)
    })

    it('draws the focus ring with the radius and color it is given', () => {
        const style = styleOf({ radius: '6.5px', ringColor: 'rgb(12, 34, 56)' })

        expect(style).toContain('border-radius:6.5px;')
        expect(style).toContain('a:focus-visible{box-shadow:inset 0 0 0 2px rgb(12, 34, 56)}')
    })

    // A computed style is where these come from, but nothing that is not a plain value may
    // reach the style sheet, where it could restyle or hide the link.
    it.each([
        ['color scheme', { colorScheme: 'dark}a{display:none' }, ':root{color-scheme:normal}'],
        ['color scheme', { colorScheme: '' }, ':root{color-scheme:normal}'],
        ['radius', { radius: '8px}a{display:none' }, 'border-radius:0;'],
        ['radius', { radius: '8px 8px 0px 0px' }, 'border-radius:0;'],
        ['ring color', { ringColor: 'red}a{display:none' }, '2px currentColor}'],
        ['ring color', { ringColor: 'url(https://example.com/x)' }, '2px currentColor}'],
    ])('falls back from an unexpected %s', (_, look, expected) => {
        const style = styleOf(look)

        expect(style).toContain(expected)
        expect(style).not.toContain('display:none')
        expect(style).not.toContain('url(')
    })
})
