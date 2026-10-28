// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Per-thread frequency channels take their own violet band, so turning them on must leave every
// other CPU color where it was, and the threads must stay inside the band in thread order.

import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import { plainToInstance } from 'class-transformer'
import { Device, DeviceType } from '@/models/Device'
import { Status } from '@/models/Status'
import { DeviceUISettings } from '@/models/UISettings'
import setDefaultSensorAndChannelColors, { threadFreqColor } from '@/stores/DeviceColorCreator'

const PACKAGE_CHANNELS = ['CPU Load', 'CPU Freq Avg', 'CPU Freq Max', 'CPU Freq Min', 'CPU Power']
// Out of order, so the numeric sort is what puts them in thread order.
const THREADS = [15, 2, 10, 0, 1, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14]

function cpuColors(threadIds: Array<number>): Map<string, string> {
    const channels = [...PACKAGE_CHANNELS, ...threadIds.map((id) => `CPU Freq Thread ${id}`)].map(
        (name) => ({ name, freq: 3000 }),
    )
    const status = plainToInstance(Status, {
        timestamp: '2026-10-28T12:00:00Z',
        temps: [{ name: 'temp1', temp: 50 }],
        channels,
    })
    const device = new Device('cpu', 'CPU', DeviceType.CPU, 1, undefined, undefined, [status])
    const settings = new Map([['cpu', new DeviceUISettings()]])
    setDefaultSensorAndChannelColors([device], settings)
    const colors = new Map<string, string>()
    for (const [name, channelSettings] of settings.get('cpu')!.sensorsAndChannels) {
        colors.set(name, channelSettings.defaultColor)
    }
    return colors
}

describe('per-thread frequency colors', () => {
    it('leave every other CPU color unchanged', () => {
        const without = cpuColors([])
        const withThreads = cpuColors(THREADS)

        for (const [name, color] of without) {
            expect(withThreads.get(name), name).toBe(color)
        }
        expect(withThreads.size).toBe(without.size + THREADS.length)
    })

    it('run through the violet band in thread order', () => {
        const colors = cpuColors(THREADS)

        expect(colors.get('CPU Freq Thread 0')).toBe('rgb(148, 82, 224)')
        expect(colors.get('CPU Freq Thread 15')).toBe('rgb(193, 38, 217)')
        expect(colors.get('CPU Freq Thread 2')).toBe(threadFreqColor(2 / 15))
        expect(colors.get('CPU Freq Thread 10')).toBe(threadFreqColor(10 / 15))
    })

    it('give a lone thread the start of the band', () => {
        expect(cpuColors([0]).get('CPU Freq Thread 0')).toBe(threadFreqColor(0))
    })
})
