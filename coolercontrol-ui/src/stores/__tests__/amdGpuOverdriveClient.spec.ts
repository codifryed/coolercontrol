// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import 'reflect-metadata'
import axios, { type AxiosAdapter, type InternalAxiosRequestConfig } from 'axios'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { AmdGpuOverdrivePlan, AmdGpuOverdriveStatus } from '@/models/AmdGpuOverdrive.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import DaemonClient from '../DaemonClient.ts'

describe('AMD GPU overdrive client', () => {
    const realAdapter = axios.defaults.adapter
    let requests: Array<InternalAxiosRequestConfig> = []

    /** Answers as a daemon would, leaving axios to build the request and parse the reply. */
    const answerWith = (status: number, body: string): void => {
        const adapter: AxiosAdapter = async (config) => {
            requests.push(config)
            const response = { data: body, status, statusText: '', headers: {}, config }
            if (status < 400) return response
            throw new axios.AxiosError('failed', 'ERR_BAD_REQUEST', config, null, response)
        }
        axios.defaults.adapter = adapter
    }

    const client = (): DaemonClient => new DaemonClient('localhost', 11987, false)

    beforeEach(() => {
        requests = []
    })

    afterEach(() => {
        axios.defaults.adapter = realAdapter
    })

    it.each([true, false])('sends rebuild %s as a JSON body', async (rebuild) => {
        answerWith(200, JSON.stringify({ state: 'applying' }))
        const result = await client().amdGpuOverdriveEnable(rebuild)
        expect(result).toBeInstanceOf(AmdGpuOverdriveStatus)
        expect(requests).toHaveLength(1)
        const [request] = requests
        expect(request.method).toBe('post')
        expect(request.url).toBe('/amd-gpu-overdrive')
        expect(request.params).toBeUndefined()
        expect(request.data).toBe(JSON.stringify({ rebuild }))
        expect(request.headers.get('Content-Type')).toBe('application/json')
    })

    it('reports a JSON refusal as an ErrorResponse', async () => {
        answerWith(409, JSON.stringify({ error: 'A setup is running' }))
        const result = await client().amdGpuOverdriveEnable(true)
        expect(result).toBeInstanceOf(ErrorResponse)
    })

    it('returns the plan', async () => {
        answerWith(200, JSON.stringify({ route: 'rebuild', files: [] }))
        const plan = await client().getAmdGpuOverdrivePlan('enable')
        expect(plan).toBeInstanceOf(AmdGpuOverdrivePlan)
    })

    it('asks for the disable plan with its action', async () => {
        answerWith(200, JSON.stringify({ route: 'config_only', files: [] }))
        await client().getAmdGpuOverdrivePlan('disable')
        expect(requests[0].params).toEqual({ action: 'disable' })
    })

    it('reports a refused plan as an ErrorResponse', async () => {
        answerWith(401, JSON.stringify({ error: 'Invalid Credentials' }))
        expect(await client().getAmdGpuOverdrivePlan('enable')).toBeInstanceOf(ErrorResponse)
    })

    it('sends a disable as a DELETE', async () => {
        answerWith(200, JSON.stringify({ state: 'applying', action: 'disable' }))
        const result = await client().amdGpuOverdriveDisable()
        expect(result).toBeInstanceOf(AmdGpuOverdriveStatus)
        expect(requests[0].method).toBe('delete')
        expect(requests[0].url).toBe('/amd-gpu-overdrive')
    })
})
