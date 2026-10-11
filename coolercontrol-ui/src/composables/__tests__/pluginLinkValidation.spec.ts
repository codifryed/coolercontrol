// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, it, expect } from 'vitest'
import { uiHostnames, validatePluginLink, validatePluginLinkForUi } from '../pluginLinkValidation'

const OWN = ['localhost']

describe('validatePluginLink', () => {
    it.each(['https://example.com/', 'http://example.com/docs?a=1&b=2#frag'])(
        'accepts %s unchanged',
        (link) => {
            expect(validatePluginLink(link, OWN)?.href).toBe(link)
        },
    )

    it('returns the normalized URL, not the raw string', () => {
        expect(validatePluginLink('  HTTPS://EXAMPLE.com\t/a b ', OWN)?.href).toBe(
            'https://example.com/a%20b',
        )
    })

    it('shows an internationalized host in its ASCII form', () => {
        expect(validatePluginLink('https://аpple.com/', OWN)?.hostname).toBe('xn--pple-43d.com')
    })

    it.each([undefined, null, 42, {}, ['https://example.com/']])('rejects non-string %j', (raw) => {
        expect(validatePluginLink(raw, OWN)).toBeNull()
    })

    it.each([
        '',
        'example.com',
        '/plugins/evil/ui/index.html',
        '//example.com/',
        'javascript:alert(1)',
        'data:text/html,<script>alert(1)</script>',
        'file:///etc/passwd',
        'ftp://example.com/',
        'mailto:user@example.com',
        'blob:https://example.com/uuid',
        'smb://example.com/share',
        'vscode://file/etc/passwd',
        'https://',
    ])('rejects %s', (link) => {
        expect(validatePluginLink(link, OWN)).toBeNull()
    })

    it.each(['https://user@example.com/', 'https://user:pass@example.com/', 'https://:p@x.com/'])(
        'rejects credentials in %s',
        (link) => {
            expect(validatePluginLink(link, OWN)).toBeNull()
        },
    )

    it('rejects a link past the length limit', () => {
        const base = 'https://example.com/'
        expect(validatePluginLink(base + 'a'.repeat(2048 - base.length), OWN)).not.toBeNull()
        expect(validatePluginLink(base + 'a'.repeat(2049 - base.length), OWN)).toBeNull()
    })

    it.each([
        'http://localhost/',
        'http://localhost:11987/plugins/evil/ui/index.html',
        'https://localhost:9999/steal',
        'http://LOCALHOST:8080/',
        'http://localhost.:8080/',
    ])('rejects the UI host on any port: %s', (link) => {
        expect(validatePluginLink(link, OWN)).toBeNull()
    })

    it.each(['http://127.0.0.1:8080/', 'http://2130706433:8080/', 'http://0x7f.1:8080/'])(
        'rejects %s when the UI is reached at 127.0.0.1',
        (link) => {
            expect(validatePluginLink(link, ['127.0.0.1'])).toBeNull()
        },
    )

    it('rejects a bracketed IPv6 UI host', () => {
        expect(validatePluginLink('http://[::1]:8080/', ['[::1]'])).toBeNull()
    })

    it('rejects every own hostname', () => {
        const own = ['localhost', 'nas.lan']
        expect(validatePluginLink('http://nas.lan:8123/', own)).toBeNull()
        expect(validatePluginLink('http://localhost:8123/', own)).toBeNull()
    })

    it('accepts another host that only resembles the UI host', () => {
        expect(validatePluginLink('http://localhost.example.com/', OWN)).not.toBeNull()
        expect(validatePluginLink('http://notlocalhost/', OWN)).not.toBeNull()
        expect(validatePluginLink('http://homeassistant.local:8123/', OWN)).not.toBeNull()
    })
})

describe('uiHostnames', () => {
    it('names the page host and the daemon host', () => {
        expect(uiHostnames('https://nas.lan:11987/')).toEqual([window.location.hostname, 'nas.lan'])
    })

    it('skips a daemon address that does not parse', () => {
        expect(uiHostnames('not a url')).toEqual([window.location.hostname])
    })
})

describe('validatePluginLinkForUi', () => {
    const DAEMON = 'https://nas.lan:11987/'

    it('rejects the page host and the daemon host', () => {
        const pageLink = `http://${window.location.hostname}:8123/`
        expect(validatePluginLinkForUi(pageLink, DAEMON)).toBeNull()
        expect(validatePluginLinkForUi('http://nas.lan:8123/', DAEMON)).toBeNull()
    })

    it('accepts another host', () => {
        expect(validatePluginLinkForUi('https://example.com/', DAEMON)?.href).toBe(
            'https://example.com/',
        )
    })
})
