/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import axios, {type AxiosInstance, type AxiosResponse} from "axios";
import axiosRetry from "axios-retry";
import {plainToInstance} from "class-transformer";
import {DeviceResponseDTO, StatusResponseDTO} from "@/stores/DataTransferModels";

/**
 * This is a Daemon Client class that handles all the direct communication with the daemon API.
 * To be used in the Device Store.
 */
export default class DaemonClient {
    private daemonURL: string = "http://127.0.0.1:11987/"
    // the daemon shouldn't take this long to respond, otherwise there's something wrong - aka not present:
    private daemonTimeout: number = 800
    private killClientTimeout: number = 1_000

    /**
     * Get the CoolerControl Daemon API Client. We generate a new instance for every call because otherwise the instance
     * holds on to the responses for its lifetime, never releasing them for GC.
     * @private
     */
    private getClient(): AxiosInstance {
        const client = axios.create({
            baseURL: this.daemonURL,
            timeout: this.daemonTimeout,
            signal: AbortSignal.timeout(this.killClientTimeout),
            withCredentials: false,
            responseType: 'json',
            transitional: {
                // `false` - throw SyntaxError if JSON parsing failed (Note: responseType must be set to 'json'):
                silentJSONParsing: false,
                clarifyTimeoutError: true,
            },
        })
        this.addRetry(client)
        return client
    }

    private addRetry(client: AxiosInstance): void {
        axiosRetry(client, {
            retries: 3,
            retryDelay: axiosRetry.exponentialDelay,
            onRetry: (retryCount) => {
                console.error("Error communicating with CoolerControl Daemon. Retry #" + retryCount)
            }
        })
    }

    private logError(err: any): void {
        console.warn(`[${new Date().toUTCString()}]\nCommunication Error: ${err}`)
    }

    private logDaemonResponse(response: AxiosResponse, name: string = "Generic"): void {
        console.debug(`[${new Date().toUTCString()}]\n${name} Response: ${response.status} ${JSON.stringify(response.data)}`)
    }

    /**
     * Makes a request handshake to confirm basic daemon connectivity.
     */
    async handshake(): Promise<boolean> {
        try {
            const response = await this.getClient().get('/handshake')
            this.logDaemonResponse(response, "Handshake")
            const handshake: { shake: boolean } = response.data
            return handshake.shake
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Requests all devices from the daemon.
     */
    async requestDevices(): Promise<DeviceResponseDTO> {
        try {
            const response = await this.getClient().get('/devices')
            this.logDaemonResponse(response, "Devices")
            return plainToInstance(DeviceResponseDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            return new DeviceResponseDTO()
        }
    }

    /**
     * requests and loads all the statuses for each device.
     */
    async completeStatusHistory(): Promise<StatusResponseDTO> {
        try {
            const response = await this.getClient().post('/status', {all: true})
            this.logDaemonResponse(response, "All Statuses")
            return plainToInstance(StatusResponseDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            console.info("This can happen when the tab goes into an inactive state.")
            return new StatusResponseDTO([])
        }
    }

    /**
     * Requests the most recent status for all devices and adds it to the current status array
     */
    async recentStatus(): Promise<StatusResponseDTO> {
        try {
            const response = await this.getClient().post('/status', {})
            this.logDaemonResponse(response, "Single Status")
            return plainToInstance(StatusResponseDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            console.info("This can happen when the tab goes into an inactive state and should be re-synced once active again.")
            return new StatusResponseDTO([])
        }
    }
}
