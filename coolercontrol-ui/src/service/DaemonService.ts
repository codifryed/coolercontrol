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
import axios, {type AxiosInstance} from 'axios'
import camelcaseKeys from "camelcase-keys";
import snakecaseKeys from "snakecase-keys";
import {plainToInstance, Type} from "class-transformer";
import axiosRetry from "axios-retry";
import type {ChannelInfo} from "@/models/ChannelInfo";
import {Status} from "@/models/Status";

export interface IDeviceService {

    /**
     * Helper function to retrieve all the devices from this service.
     */
    readonly allDevices: ReadonlyArray<Device>

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

export class DaemonService implements IDeviceService {

    public reloadAllStatuses: boolean = false

    private devices: Map<string, Device> = new Map<string, Device>()
    private scheduledEvents: any[] = []
    private updateJobInterval: number = 1
    private settingsObserver: any
    private compositeTempsEnabled: boolean = false // todo: get from settings
    private hwmonTempsEnabled: boolean = false // todo: get from settings
    private hwmonFilterEnabled: boolean = false // todo: get from settings
    private cpuCoreTempsEnabled: boolean = false // todo: get from settings
    private excludedChannelNames: Map<string, string[]> = new Map<string, string[]>()
    private client: AxiosInstance = axios.create({
        baseURL: 'http://127.0.0.1:11987/',
        timeout: 10_000,
        withCredentials: false,
        responseType: 'json',
        transitional: {
            silentJSONParsing: false, // `false` - throw SyntaxError if JSON parsing failed (Note: responseType must be set to 'json')
            clarifyTimeoutError: true,
        },
    })

    /**
     * Necessary setup logic for the service is contained here.
     * @param scheduledEvents
     * @param updateJobInternal
     */
    constructor(
        scheduledEvents: any[] = [],
        updateJobInternal: number = 1
    ) {
        this.scheduledEvents = scheduledEvents
        this.updateJobInterval = updateJobInternal
        axiosRetry(this.client, {
            retries: 3,
            retryDelay: axiosRetry.exponentialDelay,
            onRetry: (retryCount) => {
                console.error("Error communicating with CoolerControl Daemon. Retry #" + retryCount)
            }
        })
        this.client.interceptors.request.use(
            (reqConf) => {
                if (reqConf.data) {
                    reqConf.data = snakecaseKeys(reqConf.data, {deep: true})
                }
                return reqConf
            }, (err) => Promise.reject(err)
        )
        this.client.interceptors.response.use(
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
        console.debug("DeviceService created")
    }

    get allDevices(): Device[] {
        return [...this.devices.values()]
    }

    async initializeDevices(): Promise<boolean> {
        console.info("Initializing Devices")
        try {
            await this.requestHandshake()
            const dto = await this.requestDevices()
            DaemonService.sortDevices(dto);
            for (const device of dto.devices) {
                // todo: check if unknownAsetek and do appropriate handling (restart)
                DaemonService.sortChannels(device);
                // todo: handle thinkpadFanControl
                this.devices.set(device.uid, device)
            }
            await this.loadAllStatuses()
            // todo: filter devices
            // todo: update device colors (should be interesting)
            console.debug('Initialized with devices:')
            console.debug(this.devices)
            return true
        } catch (err) {
            console.error("Could not establish a connection with the daemon.")
            console.error(err)
        }
        return false
    }

    /**
     * Makes a request handshake to confirm basic daemon connectivity.
     * @private
     */
    private async requestHandshake(): Promise<void> {
        const response = await this.client.get('/handshake')
        console.debug("Handshake response: " + JSON.stringify(response.data))
        if (!response.data.shake) {
            throw new Error("Incorrect handshake response: " + JSON.stringify(response.data))
        }
        console.info("Daemon handshake successful")
    }

    /**
     * Requests all devices from the daemon.
     * @private
     */
    private async requestDevices(): Promise<DeviceResponseDTO> {
        const response = await this.client.get('/devices')
        console.debug("Get Devices RAW Response received:")
        console.debug(response.data)
        if (Array.isArray(response.data)) {
            throw new Error("Devices Response was an Array!")
        }
        const dto = plainToInstance(DeviceResponseDTO, response.data as object)
        console.debug("Device Response PARSED:")
        console.debug(dto)
        console.info('Devices successfully initialized')
        return dto
    }

    /**
     * requests and loads all the statuses for each device.
     */
    async loadAllStatuses(): Promise<void> {
        const response = await this.client.post('/status', {all: true})
        console.debug("All Status Response received:")
        console.debug(response.data)
        if (!response.data || Array.isArray(response.data)) {
            throw new Error("All Status Response was empty or an Array!")
        }
        const dto = plainToInstance(StatusResponseDTO, response.data as object)
        for (const dtoDevice of dto.devices) {
            // not all device UIDs are present locally (composite can be ignored for example)
            if (this.devices.has(dtoDevice.uid)) {
                this.devices.get(dtoDevice.uid)!.statusHistory.length = 0 // clear array
                this.devices.get(dtoDevice.uid)!.statusHistory.push(...dtoDevice.statusHistory)
            }
        }
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
            } else if (a.typeIndex > b.typeIndex) {
                return 1
            } else if (a.typeIndex < b.typeIndex) {
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

    shutdown(): void {
        this.devices.clear()
        console.debug("CoolerControl Daemon Service shutdown")
    }

    /**
     * Requests the most recent status for all devices and adds it to the current status array
     */
    async updateStatus(): Promise<boolean> {
        let onlyLatestStatusShouldBeUpdated: boolean = true
        let timeDiffMillis: number = 0;
        const response = await this.client.post('/status', {})
        console.debug("Single Status Response received:")
        console.debug(response.data)
        if (!response.data || Array.isArray(response.data)) {
            throw new Error("All Status Response was empty or an Array!")
        }
        const dto = plainToInstance(StatusResponseDTO, response.data as object)

        if (dto.devices.length > 0 && this.devices.size > 0) {
            const device: Device = this.devices.values().next().value
            timeDiffMillis = Math.abs(
                device.status.timestamp.getTime() - dto.devices[0].statusHistory[0].timestamp.getTime()
            )
            if (timeDiffMillis > 2000) {
                onlyLatestStatusShouldBeUpdated = false
            }
        }

        if (onlyLatestStatusShouldBeUpdated) {
            for (const dtoDevice of dto.devices) {
                // not all device UIDs are present locally (composite can be ignored for example)
                if (this.devices.has(dtoDevice.uid)) {
                    const statuses = this.devices.get(dtoDevice.uid)!.statusHistory
                    statuses.push(...dtoDevice.statusHistory)
                    statuses.shift()
                }
            }
        } else {
            console.warn(`Device Statuses are out of sync by ${timeDiffMillis}ms, refreshing all.`)
            await this.loadAllStatuses()
        }
        return onlyLatestStatusShouldBeUpdated
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
    typeIndex: TypeIndex

    @Type(() => Status)
    statusHistory: Status[]

    constructor(
        type: DeviceType,
        typeIndex: TypeIndex,
        uid: UID,
        statusHistory: Status[]
    ) {
        this.type = type
        this.typeIndex = typeIndex
        this.uid = uid
        this.statusHistory = statusHistory
    }
}