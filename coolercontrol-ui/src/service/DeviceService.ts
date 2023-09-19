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

import {Device, DeviceType, type TypeIndex, type UID} from "@/models/Device"
import axios, {type AxiosInstance, type AxiosResponse} from 'axios'
import {plainToInstance, Type} from "class-transformer";
import type {ChannelInfo} from "@/models/ChannelInfo";
import {Status} from "@/models/Status";
import camelcaseKeys from "camelcase-keys";
import snakecaseKeys from "snakecase-keys";
import axiosRetry from "axios-retry";

export interface IDeviceService {

    /**
     * Helper function to retrieve all the devices from this service.
     */
    // readonly allDevices: ReadonlyArray<Device>
    readonly allDevices: IterableIterator<Device>

    /**
     * Initializes all devices.
     * There are usually several steps involved before devices are ready to be consumed by the components.
     */
    initializeDevices(): void;

    /**
     * Called regularly to update the status of each device.
     */
    updateStatus(): void;

    /**
     * Called on application shutdown.
     */
    shutdown(): void;
}

export class DeviceService implements IDeviceService {

    public reloadAllStatuses: boolean = false

    private daemonClient: DaemonClient
    private devices: Map<string, Device> = new Map<string, Device>()
    private updateJobInterval: number = 1
    private reloadAllStatusesThreshold: number = 2_000
    private settingsObserver: any
    private compositeTempsEnabled: boolean = false // todo: get from settings
    private hwmonTempsEnabled: boolean = false // todo: get from settings
    private hwmonFilterEnabled: boolean = false // todo: get from settings
    private cpuCoreTempsEnabled: boolean = false // todo: get from settings
    private excludedChannelNames: Map<string, string[]> = new Map<string, string[]>()

    /**
     * Necessary setup logic for the service is contained here.
     * @param updateJobInternal
     */
    constructor(
        updateJobInternal: number = 1
    ) {
        this.updateJobInterval = updateJobInternal
        this.daemonClient = new DaemonClient()
        console.debug("DeviceService created")
    }

    get allDevices(): IterableIterator<Device> {
        return this.devices.values()
    }

    async initializeDevices(): Promise<boolean> {
        console.info("Initializing Devices")
        const handshakeSuccessful = await this.daemonClient.handshake()
        if (!handshakeSuccessful) {
            return false
        }
        const dto = await this.daemonClient.requestDevices()
        if (dto.devices.length === 0) {
            console.warn("There are no available devices!")
        }
        DeviceService.sortDevices(dto);
        for (const device of dto.devices) {
            // todo: check if unknownAsetek and do appropriate handling (restart)
            DeviceService.sortChannels(device);
            // todo: handle thinkpadFanControl
            this.devices.set(device.uid, device)
        }
        await this.loadCompleteStatusHistory()
        // todo: filter devices
        // todo: update device colors (should be interesting)
        // todo:use d3 color lib?
        console.debug('Initialized with devices:')
        console.debug(this.devices)
        return true
    }

    /**
     * requests and loads all the statuses for each device.
     */
    async loadCompleteStatusHistory(): Promise<void> {
        const allStatusesDto = await this.daemonClient.completeStatusHistory()
        for (const dtoDevice of allStatusesDto.devices) {
            // not all device UIDs are present locally (composite can be ignored for example)
            if (this.devices.has(dtoDevice.uid)) {
                const statuses = this.devices.get(dtoDevice.uid)!.status_history
                statuses.length = 0 // clear array if this is a re-sync
                statuses.push(...dtoDevice.status_history) // shallow copy
            }
        }
    }


    /**
     * Requests the most recent status for all devices and adds it to the current status array
     */
    async updateStatus(): Promise<boolean> {
        let onlyLatestStatusShouldBeUpdated: boolean = true
        let timeDiffMillis: number = 0;
        const dto = await this.daemonClient.recentStatus()
        if (dto.devices.length > 0 && this.devices.size > 0) {
            const device: Device = this.devices.values().next().value
            timeDiffMillis = Math.abs(
                new Date(device.status.timestamp).getTime()
                - new Date(dto.devices[0].status_history[0].timestamp).getTime()
            )
            if (timeDiffMillis > this.reloadAllStatusesThreshold) {
                onlyLatestStatusShouldBeUpdated = false
            }
        }

        if (onlyLatestStatusShouldBeUpdated) {
            for (const dtoDevice of dto.devices) {
                // not all device UIDs are present locally (composite can be ignored for example)
                if (this.devices.has(dtoDevice.uid)) {
                    const statuses = this.devices.get(dtoDevice.uid)!.status_history
                    statuses.push(...dtoDevice.status_history)
                    // todo: verify that the new status is indeed "new" / timestamp != last timestamp:
                    //  AND that the size of the array hasn't reached it's theoretical maximum (1860)
                    statuses.shift()
                }
            }
        } else {
            console.info(`[${new Date().toUTCString()}]:\nDevice Statuses are out of sync by ${new Intl.NumberFormat().format(timeDiffMillis)}ms, reloading all.`)
            await this.loadCompleteStatusHistory()
        }
        return onlyLatestStatusShouldBeUpdated
    }

    shutdown(): void {
        this.devices.clear()
        console.debug("CoolerControl Daemon Service shutdown")
    }

    /**
     * Sorts the devices in the DeviceResponseDTO by first type, and then by typeIndex
     * @param dto
     * @private
     */
    private static sortDevices(dto: DeviceResponseDTO): void {
        dto.devices.sort((a, b) => {
            if (a.type > b.type) {
                return 1
            } else if (a.type < b.type) {
                return -1
            } else if (a.type_index > b.type_index) {
                return 1
            } else if (a.type_index < b.type_index) {
                return -1
            } else {
                return 0
            }
        })
    }

    /**
     * Sorts channels by channel name
     * @param device
     * @private
     */
    private static sortChannels(device: Device): void {
        if (device.info?.channels) {
            device.info.channels = new Map<string, ChannelInfo>([...device.info.channels.entries()].sort())
        }
    }
}

class DaemonClient {
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
        // this.addCaseTransformers(client) // keep objects that come directly from the daemon snake_case
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

    private addCaseTransformers(client: AxiosInstance): void {
        client.interceptors.request.use(
            (reqConf) => {
                if (reqConf.data) {
                    reqConf.data = snakecaseKeys(reqConf.data, {deep: true})
                }
                return reqConf
            }, (err) => Promise.reject(err)
        )
        client.interceptors.response.use(
            (response) => {
                if (response.data) {
                    response.data = camelcaseKeys(response.data, {deep: true})
                }
                return response
            }, (err) => {
                if (err.data) {
                    err.data = camelcaseKeys(err.data, {deep: true})
                }
                return Promise.reject(err)
            }
        )
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
            // We use raw json here as much as possible to avoid memory leaks (instantiating classes for every status update seems to lot let them go for GC)
            // const dto = JSON.parse(JSON.stringify(response.data)) as StatusResponseDTO
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
            // const dto = plainToInstance(StatusResponseDTO, response.data as object)
            // const dto = response.data as StatusResponseDTO
            return plainToInstance(StatusResponseDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            console.info("This can happen when the tab goes into an inactive state and should be re-synced once active again.")
            return new StatusResponseDTO([])
        }
    }
}

class DeviceResponseDTO {

    @Type(() => Device)
    public devices: Device[];

    constructor(
        devices: Device[] = []
    ) {
        this.devices = devices;
    }
}

class StatusResponseDTO {

    @Type(() => DeviceStatusDTO)
    devices: DeviceStatusDTO[]

    constructor(
        devices: DeviceStatusDTO[]
    ) {
        this.devices = devices
    }
}

class DeviceStatusDTO {

    uid: UID
    type: DeviceType
    type_index: TypeIndex

    @Type(() => Status)
    status_history: Status[]

    constructor(
        type: DeviceType,
        type_index: TypeIndex,
        uid: UID,
        status_history: Status[]
    ) {
        this.type = type
        this.type_index = type_index
        this.uid = uid
        this.status_history = status_history
    }
}