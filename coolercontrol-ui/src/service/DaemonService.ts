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
    readonly allDevices: ReadonlyArray<Device>

    initializeDevices(): void;

    updateStatus(): void;

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

    private async requestHandshake(): Promise<void> {
        const response = await this.client.get('/handshake')
        console.debug("Handshake response: " + JSON.stringify(response.data))
        if (!response.data.shake) {
            throw new Error("Incorrect handshake response: " + JSON.stringify(response.data))
        }
        console.info("Daemon handshake successful")
    }

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

    private async loadAllStatuses(): Promise<void> {
        const response = await this.client.post('/status', {all: true})
        console.debug("All Status Response received:")
        console.debug(response.data)
        if (!response.data || Array.isArray(response.data)) {
            throw new Error("All Status Response was empty or an Array!")
        }
        const dto = plainToInstance(StatusResponseDTO, response.data as object)
        for (const device of dto.devices) {
            // not all device UIDs are present locally (composite can be ignored for example)
            if (this.devices.has(device.uid)) {
                this.devices.get(device.uid)!.statusHistory = device.statusHistory
            }
        }
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

    updateStatus(): void {
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

    @Type(() => StatusDTO)
    devices: StatusDTO[]

    constructor(
            devices: StatusDTO[]
    ) {
        this.devices = devices
    }
}

class StatusDTO {

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