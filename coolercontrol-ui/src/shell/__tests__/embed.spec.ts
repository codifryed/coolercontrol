// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { notifyEmbedder, requestEmbedderPalette } from '../embed.ts'

const PALETTE = {
    variant: 'dark',
    tokens: {
        accent: '#b9dafc',
        accentGradientTo: '#b9dafc',
        bgOne: '#292929',
        bgTwo: '#151515',
        borderOne: '#4d4d4d',
        textColor: '#ffffff',
        textColorSecondary: '#c7c7c7',
        success: '#87bb62',
        warning: '#ffcc17',
        error: '#f89b78',
        info: '#b6a6e9',
    },
}

/** A window other than this one, as a stand-in for a page or a frame. */
const otherWindow = () => ({ postMessage: vi.fn() }) as unknown as Window

describe('embedding page messages', () => {
    let embedder: Window
    let apply: ReturnType<typeof vi.fn>
    let errors: Array<unknown>
    const cleanups: Array<() => void> = []

    /** Delivers a message the way the browser does. The embedding page sends unless told otherwise. */
    const receive = (data: unknown, from: { source?: Window; origin?: string } = {}): void => {
        window.dispatchEvent(
            new MessageEvent('message', {
                data,
                source: from.source ?? embedder,
                origin: from.origin ?? window.location.origin,
            }),
        )
    }

    const embed = (): void => {
        vi.spyOn(window, 'parent', 'get').mockReturnValue(embedder)
    }

    beforeEach(() => {
        embedder = otherWindow()
        apply = vi.fn()
        errors = []
        // The listener lives as long as the page does, so take it down by hand.
        const addEventListener = window.addEventListener.bind(window)
        vi.spyOn(window, 'addEventListener').mockImplementation((type, listener, options) => {
            addEventListener(type, listener, options)
            cleanups.push(() => window.removeEventListener(type, listener, options))
        })
        // A listener that throws does not fail the dispatch: the window reports it.
        window.addEventListener('error', (event) => {
            errors.push(event.error)
            event.preventDefault()
        })
        vi.spyOn(console, 'error').mockImplementation(() => {})
    })

    afterEach(() => {
        for (const cleanup of cleanups.splice(0)) cleanup()
        vi.restoreAllMocks()
    })

    /// Goal: the notices an integration listens for keep their names and shape.
    it.each([
        ['load-start', 'coolercontrol:load-start'],
        ['palette-request', 'coolercontrol:palette-request'],
        ['load-end', 'coolercontrol:load-end'],
    ] as const)('tells the embedding page of %s', (notice, type) => {
        embed()
        notifyEmbedder(notice)

        expect(embedder.postMessage).toHaveBeenCalledTimes(1)
        expect(embedder.postMessage).toHaveBeenCalledWith({ type }, '*')
    })

    /// Goal: a browser tab and the Qt app are not embedded, and stay silent.
    it('does nothing outside a frame', () => {
        const post = vi.spyOn(window, 'postMessage')
        notifyEmbedder('load-start')
        requestEmbedderPalette(apply)
        receive({ type: 'coolercontrol:palette', palette: PALETTE }, { source: window })

        expect(post).not.toHaveBeenCalled()
        expect(apply).not.toHaveBeenCalled()
    })

    /// Goal: the request an integration answers keeps its name and shape.
    it('asks the embedding page for its palette', () => {
        embed()
        requestEmbedderPalette(apply)

        expect(embedder.postMessage).toHaveBeenCalledTimes(1)
        expect(embedder.postMessage).toHaveBeenCalledWith(
            { type: 'coolercontrol:palette-request' },
            '*',
        )
    })

    /// Goal: the palette message an integration sends keeps working as documented.
    it('hands over the palette the embedding page sends', () => {
        embed()
        requestEmbedderPalette(apply)
        receive({ type: 'coolercontrol:palette', palette: PALETTE })

        expect(apply).toHaveBeenCalledTimes(1)
        expect(apply).toHaveBeenCalledWith(PALETTE)
    })

    /// Goal: the embedding page can follow its own theme by sending again.
    it('hands over every palette it is sent', () => {
        embed()
        requestEmbedderPalette(apply)
        receive({ type: 'coolercontrol:palette', palette: PALETTE })
        receive({ type: 'coolercontrol:palette', palette: { variant: 'light' } })

        expect(apply).toHaveBeenCalledTimes(2)
        expect(apply).toHaveBeenLastCalledWith({ variant: 'light' })
    })

    /// Goal: the embedding page is served on its own port, such as a dashboard's.
    it('hears the embedding page on another port', () => {
        embed()
        requestEmbedderPalette(apply)
        const { protocol, hostname } = window.location
        receive(
            { type: 'coolercontrol:palette', palette: PALETTE },
            { origin: `${protocol}//${hostname}:9090` },
        )

        expect(apply).toHaveBeenCalledTimes(1)
    })

    /// Goal: only the page directly above restyles the UI, not a sibling frame
    /// or a plugin frame of the UI's own, whatever origin it has.
    it('ignores a palette from any other window', () => {
        embed()
        requestEmbedderPalette(apply)
        receive({ type: 'coolercontrol:palette', palette: PALETTE }, { source: otherWindow() })
        receive({ type: 'coolercontrol:palette', palette: PALETTE }, { source: window })

        expect(apply).not.toHaveBeenCalled()
    })

    /// Goal: a sandboxed sender, whose origin is the string "null", is turned
    /// away quietly. Plugin frames are sandboxed and post to this window often.
    it.each([
        ['a plugin frame', () => otherWindow()],
        ['the embedding page', () => embedder],
    ])('ignores a sandboxed sender: %s', (_, source) => {
        embed()
        requestEmbedderPalette(apply)
        receive(
            { type: 'coolercontrol:palette', palette: PALETTE },
            { source: source(), origin: 'null' },
        )
        receive(
            { type: 'openLink', body: 'https://example.com/' },
            { source: source(), origin: 'null' },
        )

        expect(apply).not.toHaveBeenCalled()
        expect(errors).toEqual([])
    })

    /// Goal: an embedding page on another host or scheme does not restyle the UI.
    it.each([
        ['hostname', () => `${window.location.protocol}//example.com:${window.location.port}`],
        ['scheme', () => `ftp://${window.location.host}`],
    ])('ignores an embedding page on another %s', (_, origin) => {
        embed()
        requestEmbedderPalette(apply)
        receive({ type: 'coolercontrol:palette', palette: PALETTE }, { origin: origin() })

        expect(apply).not.toHaveBeenCalled()
        expect(errors).toEqual([])
    })

    /// Goal: whatever else the embedding page posts is left alone, without an error.
    it.each([
        ['nothing', null],
        ['nothing defined', undefined],
        ['a string', 'coolercontrol:palette'],
        ['a number', 42],
        ['an object with no type', { palette: PALETTE }],
        ['another message', { type: 'coolercontrol:other', palette: PALETTE }],
    ])('ignores %s', (_, data) => {
        embed()
        requestEmbedderPalette(apply)
        receive(data)

        expect(apply).not.toHaveBeenCalled()
        expect(errors).toEqual([])
    })

    /// Goal: a palette with nothing usable in it never replaces the one in use.
    it.each([
        ['missing', undefined],
        ['a string', JSON.stringify(PALETTE)],
        ['empty', {}],
        ['of an unknown variant', { variant: 'sepia' }],
    ])('does not hand over a palette that is %s', (_, palette) => {
        embed()
        requestEmbedderPalette(apply)
        receive({ type: 'coolercontrol:palette', palette })

        expect(apply).not.toHaveBeenCalled()
        expect(errors).toEqual([])
    })
})
