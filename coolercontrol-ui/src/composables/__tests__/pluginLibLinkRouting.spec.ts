// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The plugin library hears every click on a plugin page. These tests pin which link clicks
// it hands to CoolerControl and which it leaves to the page.

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

const LIBRARY_PATH = resolve(
    __dirname,
    '../../../../coolercontrold/daemon/resources/lib/cc-plugin-lib.js',
)

const postMessage = vi.fn()
const realParent = Object.getOwnPropertyDescriptor(window, 'parent')!

function setFramed(isFramed: boolean): void {
    Object.defineProperty(window, 'parent', {
        value: isFramed ? { postMessage } : window,
        configurable: true,
    })
}

/** Click a link as a user would, and report whether the library took the click over. */
function click(href: string, init: MouseEventInit = {}, inner = ''): boolean {
    const anchor = document.createElement('a')
    anchor.setAttribute('href', href)
    anchor.innerHTML = inner
    document.body.appendChild(anchor)
    const target = anchor.firstElementChild ?? anchor
    const event = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0, ...init })
    // After the library has seen the click, stop it: jsdom cannot navigate.
    let wasTaken = false
    window.addEventListener(
        'click',
        (seen) => {
            wasTaken = seen.defaultPrevented
            seen.preventDefault()
        },
        { once: true },
    )
    target.dispatchEvent(event)
    return wasTaken
}

beforeAll(() => {
    // The library is a classic script, as a plugin page loads it.
    new Function(readFileSync(LIBRARY_PATH, 'utf8'))()
})
afterAll(() => {
    Object.defineProperty(window, 'parent', realParent)
})
afterEach(() => {
    document.body.innerHTML = ''
    postMessage.mockClear()
})

describe('cc-plugin-lib link routing', () => {
    it('hands a link to another host to CoolerControl', () => {
        setFramed(true)

        expect(click('https://example.com/docs?a=1')).toBe(true)
        expect(postMessage).toHaveBeenCalledExactlyOnceWith(
            { type: 'openLink', url: 'https://example.com/docs?a=1' },
            document.location.origin,
        )
    })

    it('hands over a click on an element inside the link', () => {
        setFramed(true)

        expect(click('http://example.com/', {}, '<span>docs</span>')).toBe(true)
        expect(postMessage).toHaveBeenCalledExactlyOnceWith(
            { type: 'openLink', url: 'http://example.com/' },
            document.location.origin,
        )
    })

    it.each([
        [
            'an absolute link to its own host',
            () => `${document.location.origin}/plugins/p/ui/b.html`,
        ],
        ['a relative link', () => 'other.html'],
        ['a fragment', () => '#section'],
        ['a mail link', () => 'mailto:someone@example.com'],
        ['a script link', () => 'javascript:void(0)'],
    ])('leaves %s to the page', (_, href) => {
        setFramed(true)

        expect(click(href())).toBe(false)
        expect(postMessage).not.toHaveBeenCalled()
    })

    it('leaves a click with another mouse button to the page', () => {
        setFramed(true)

        expect(click('https://example.com/', { button: 1 })).toBe(false)
        expect(postMessage).not.toHaveBeenCalled()
    })

    it('leaves a click the page already handled alone', () => {
        setFramed(true)
        document.body.addEventListener('click', (event) => event.preventDefault(), { once: true })

        click('https://example.com/')
        expect(postMessage).not.toHaveBeenCalled()
    })

    it('leaves a click outside any link alone', () => {
        setFramed(true)
        const event = new MouseEvent('click', { bubbles: true, cancelable: true })
        document.body.dispatchEvent(event)

        expect(event.defaultPrevented).toBe(false)
        expect(postMessage).not.toHaveBeenCalled()
    })

    it('leaves every link to a page that is not framed', () => {
        setFramed(false)

        expect(click('https://example.com/')).toBe(false)
        expect(postMessage).not.toHaveBeenCalled()
    })
})
