// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// devices.ts pulls in Device.ts, whose class-transformer decorators need the polyfill.
import 'reflect-metadata'
import { describe, expect, it } from 'vitest'
import { Device, DeviceType } from '@/models/Device.ts'
import { DeviceInfo } from '@/models/DeviceInfo.ts'
import { ChannelInfo } from '@/models/ChannelInfo.ts'
import { SpeedOptions } from '@/models/SpeedOptions.ts'
import { Status, TempStatus } from '@/models/Status.ts'
import { TempInfo } from '@/models/TempInfo.ts'
import { buildIndex, type IndexDeps } from '../index.ts'
import type { SearchKind } from '../types.ts'

const fanChannel = (): ChannelInfo => new ChannelInfo('Fan 1', new SpeedOptions(0, 100, true))

function device(over: Partial<Device> = {}): Device {
    const info = new DeviceInfo()
    info.channels = new Map([['fan1', fanChannel()]])
    info.temps = new Map([['temp1', new TempInfo('CPU Temp', 1)]])
    return {
        uid: 'uid-1',
        name: 'Board',
        type: DeviceType.HWMON,
        type_index: 1,
        info,
        status: new Status('t0'),
        status_history: [],
        ...over,
    } as Device
}

function deps(over: Partial<IndexDeps> = {}): IndexDeps {
    return {
        devices: [device()],
        deviceLabel: (uid) => (uid === 'uid-1' ? 'My Board' : uid),
        channelLabel: (_uid, channelName) => channelName,
        channelUnit: () => undefined,
        profiles: [],
        functions: [],
        modes: [],
        dashboards: [],
        alerts: [],
        pluginIds: [],
        isQtApp: false,
        t: (key) => key,
        tEn: (key) => key,
        ...over,
    }
}

const kinds = (entries: ReturnType<typeof buildIndex>): Set<SearchKind> =>
    new Set(entries.map((entry) => entry.kind))

describe('buildIndex', () => {
    it('classifies a speed channel as a fan and sends it to its Cooling page', () => {
        const fan = buildIndex(deps()).find((entry) => entry.kind === 'fan')
        expect(fan?.id).toBe('fan-uid-1-fan1')
        expect(fan?.target).toEqual({
            route: { name: 'cooling-channel', params: { deviceUID: 'uid-1', channelName: 'fan1' } },
        })
    })

    // A fan is listed once, under Cooling. Repeating it under Sensors would put
    // the same destination in two groups of the same result list.
    it('does not repeat a fan as a sensor', () => {
        const ids = buildIndex(deps()).map((entry) => entry.id)
        expect(ids).toContain('fan-uid-1-fan1')
        expect(ids).not.toContain('sensor-uid-1-fan1')
    })

    // A flow meter on a fan input is watched, not controlled.
    it('indexes an uncontrollable channel in another unit as a sensor', () => {
        const info = new DeviceInfo()
        info.channels = new Map([
            ['fan9', new ChannelInfo('Flow [dL/h]', new SpeedOptions(0, 100, false))],
        ])
        const entries = buildIndex(deps({ devices: [device({ info })], channelUnit: () => 'dL/h' }))
        const ids = entries.map((entry) => entry.id)
        expect(ids).not.toContain('fan-uid-1-fan9')
        const sensor = entries.find((entry) => entry.id === 'sensor-uid-1-fan9')
        expect(sensor?.target).toEqual({
            route: {
                name: 'monitoring-sensor',
                params: { deviceUID: 'uid-1', channelName: 'fan9' },
            },
        })
    })

    it('indexes a temp as a sensor', () => {
        const sensor = buildIndex(deps()).find((entry) => entry.id === 'sensor-uid-1-temp1')
        expect(sensor?.kind).toBe('sensor')
    })

    it('gives custom sensors their own kind and the Devices breadcrumb', () => {
        const info = new DeviceInfo()
        info.temps = new Map([['sensor1', new TempInfo('Mix', 1)]])
        const custom = device({
            uid: 'cs',
            name: 'Custom Sensors',
            type: DeviceType.CUSTOM_SENSORS,
            info,
        })
        const entries = buildIndex(deps({ devices: [custom] }))
        const entry = entries.find((candidate) => candidate.kind === 'customSensor')
        expect(entry?.id).toBe('custom-cs-sensor1')
        expect(entry?.breadcrumb[0]).toBe('layout.shell.devices')
    })

    it('uses the display name, so a user rename is searchable', () => {
        const entries = buildIndex(
            deps({ channelLabel: () => 'Front Intake', deviceLabel: () => 'My Board' }),
        )
        const fan = entries.find((entry) => entry.kind === 'fan')
        expect(fan?.label).toBe('Front Intake')
        expect(fan?.breadcrumb).toEqual(['layout.shell.cooling', 'My Board'])
    })

    // The palette is closed while status streams, so the index must not depend
    // on it; if it did, every poll tick would invalidate the build.
    it('ignores device status entirely', () => {
        const target = device()
        const before = buildIndex(deps({ devices: [target] }))
        target.status.temps.push(new TempStatus('temp1', 42))
        ;(target.status_history as Status[]).push(new Status('t1'))
        const after = buildIndex(deps({ devices: [target] }))
        expect(after).toEqual(before)
    })

    it('indexes the library entities it is given', () => {
        const entries = buildIndex(
            deps({
                profiles: [{ uid: 'p1', name: 'Silent Curve' }],
                functions: [{ uid: 'f1', name: 'Smooth' }],
                modes: [{ uid: 'm1', name: 'Quiet' }],
                dashboards: [{ uid: 'd1', name: 'Temps' }],
                alerts: [{ uid: 'a1', name: 'Fan Fail' }],
            }),
        )
        expect(kinds(entries)).toContain('profile')
        const profile = entries.find((entry) => entry.id === 'profile-p1')
        expect(profile?.label).toBe('Silent Curve')
        expect(profile?.target).toEqual({
            route: { name: 'profiles', params: { profileUID: 'p1' } },
        })
    })

    it('routes a settings row through its own anchor id', () => {
        const entry = buildIndex(deps()).find(
            (candidate) => candidate.id === 'setting-polling-rate',
        )
        expect(entry?.target).toEqual({
            route: { name: 'settings', params: { tabNumber: 'setting-polling-rate' } },
        })
        expect(entry?.keywords).toContain('interval')
    })

    it('carries the card and group in the breadcrumb', () => {
        const entry = buildIndex(deps()).find(
            (candidate) => candidate.id === 'setting-polling-rate',
        )
        expect(entry?.breadcrumb).toEqual([
            'layout.shell.settings',
            'views.daemon.title',
            'layout.settings.groups.performance',
        ])
    })

    it('holds back desktop-only entries outside the Qt app', () => {
        const web = buildIndex(deps()).map((entry) => entry.id)
        const qt = buildIndex(deps({ isQtApp: true })).map((entry) => entry.id)
        expect(web).not.toContain('setting-start-in-tray')
        expect(web).not.toContain('action-open-in-browser')
        expect(web).not.toContain('page-settings-desktop')
        expect(qt).toContain('setting-start-in-tray')
        expect(qt).toContain('action-open-in-browser')
    })

    it('holds back the plugins page until a plugin is loaded', () => {
        expect(buildIndex(deps()).map((entry) => entry.id)).not.toContain('page-plugins')
        const withPlugin = buildIndex(deps({ pluginIds: ['fan-hub'] }))
        expect(withPlugin.map((entry) => entry.id)).toContain('page-plugins')
        expect(withPlugin.find((entry) => entry.kind === 'plugin')?.label).toBe('fan-hub')
    })

    it('indexes actions as actions, not routes', () => {
        const entry = buildIndex(deps()).find(
            (candidate) => candidate.id === 'action-calibrate-fans',
        )
        expect(entry?.target).toEqual({ action: 'calibrate-fans' })
    })

    it('gives every entry a unique id', () => {
        const ids = buildIndex(deps({ isQtApp: true, pluginIds: ['p'] })).map((entry) => entry.id)
        expect(new Set(ids).size).toBe(ids.length)
    })
})
