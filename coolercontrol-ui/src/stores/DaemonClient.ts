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

import axios, { type AxiosInstance, type AxiosResponse } from 'axios'
import axiosRetry from 'axios-retry'
import { instanceToPlain, plainToInstance } from 'class-transformer'
import { DeviceResponseDTO, StatusResponseDTO } from '@/stores/DataTransferModels'
import { UISettingsDTO } from '@/models/UISettings'
import type { UID } from '@/models/Device'
import {
    DeviceSettingsReadDTO,
    DeviceSettingWriteLcdDTO,
    DeviceSettingWriteLightingDTO,
    DeviceSettingWriteManualDTO,
    DeviceSettingWriteProfileDTO,
    DeviceSettingWritePWMModeDTO,
} from '@/models/DaemonSettings'
import { Function, FunctionsDTO, Profile, ProfilesDTO } from '@/models/Profile'
import { ErrorResponse } from '@/models/ErrorResponse'
import {
    CoolerControlAllDeviceSettingsDTO,
    CoolerControlDeviceSettingsDTO,
    CoolerControlSettingsDTO,
} from '@/models/CCSettings'
import { CustomSensor } from '@/models/CustomSensor'

/**
 * This is a Daemon Client class that handles all the direct communication with the daemon API.
 * To be used in the Device Store.
 */
export default class DaemonClient {
    private daemonURL: string = 'http://127.0.0.1:11987/'
    // the daemon shouldn't take this long to respond, otherwise there's something wrong - aka not present:
    private daemonTimeout: number = 800
    private daemonTimeoutExtended: number = 8_000 // this is for image processing calls that can take significantly longer
    private daemonInitialConnectionTimeout: number = 30_000 // to allow extra time for the daemon to come up
    private killClientTimeout: number = 1_000
    private killClientTimeoutExtended: number = 10_000 // this is for image processing calls that can take significantly longer
    private responseLogging: boolean = false

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
            retries: 2,
            shouldResetTimeout: false,
            retryDelay: axiosRetry.exponentialDelay,
            onRetry: (retryCount) => {
                console.error('Error communicating with CoolerControl Daemon. Retry #' + retryCount)
            },
        })
    }

    private logError(err: any): void {
        console.warn(`[${new Date().toUTCString()}]\nCommunication Error: ${err}`)
    }

    private logDaemonResponse(response: AxiosResponse, name: string = 'Generic'): void {
        if (this.responseLogging) {
            console.debug(
                `[${new Date().toUTCString()}]\n${name} Response: ${
                    response.status
                } ${JSON.stringify(response.data)}`,
            )
        }
    }

    /**
     * Makes a request handshake to confirm basic daemon connectivity.
     */
    async handshake(): Promise<boolean> {
        try {
            const response = await this.getClient().get('/handshake', {
                // first connection attempt should work harder:
                timeout: this.daemonInitialConnectionTimeout,
                signal: AbortSignal.timeout(this.daemonInitialConnectionTimeout),
                'axios-retry': {
                    retries: 10,
                },
            })
            this.logDaemonResponse(response, 'Handshake')
            const handshake: {
                shake: boolean
            } = response.data
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
            this.logDaemonResponse(response, 'Devices')
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
            const response = await this.getClient().post('/status', { all: true })
            this.logDaemonResponse(response, 'All Statuses')
            return plainToInstance(StatusResponseDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            console.info('This can happen when the tab goes into an inactive state.')
            return new StatusResponseDTO([])
        }
    }

    /**
     * Requests the most recent status for all devices and adds it to the current status array
     */
    async recentStatus(): Promise<StatusResponseDTO> {
        try {
            const response = await this.getClient().post('/status', {})
            this.logDaemonResponse(response, 'Single Status')
            return plainToInstance(StatusResponseDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            console.info(
                'This can happen when the tab goes into an inactive state and should be re-synced once active again.',
            )
            return new StatusResponseDTO([])
        }
    }

    /**
     * Sends the UI Settings to the daemon for persistence.
     * @param uiSettings {UISettingsDTO}
     */
    async saveUISettings(uiSettings: UISettingsDTO): Promise<boolean> {
        try {
            const response = await this.getClient().put('/settings/ui', instanceToPlain(uiSettings))
            this.logDaemonResponse(response, 'Save UI Settings')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Retrieves the persisted UI Settings from the daemon.
     * @returns {UISettingsDTO}
     */
    async loadUISettings(): Promise<UISettingsDTO> {
        try {
            const response = await this.getClient().get('/settings/ui')
            this.logDaemonResponse(response, 'Load UI Settings')
            return plainToInstance(UISettingsDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            return new UISettingsDTO()
        }
    }

    /**
     * Retrieves general CoolerControl settings from the daemon
     */
    async loadCCSettings(): Promise<CoolerControlSettingsDTO> {
        try {
            const response = await this.getClient().get('/settings')
            this.logDaemonResponse(response, 'Load CC Settings')
            return plainToInstance(CoolerControlSettingsDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            return new CoolerControlSettingsDTO()
        }
    }

    /**
     * Persists and applies general CoolerControl settings
     */
    async saveCCSettings(ccSettings: CoolerControlSettingsDTO): Promise<boolean> {
        try {
            const response = await this.getClient().patch('/settings', instanceToPlain(ccSettings))
            this.logDaemonResponse(response, 'Save CC Settings')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Retrieves general CoolerControl settings for All Devices from the daemon
     */
    async loadCCAllDeviceSettings(): Promise<CoolerControlAllDeviceSettingsDTO> {
        try {
            const response = await this.getClient().get('/settings/devices')
            this.logDaemonResponse(response, 'Load CC Settings for All devices')
            return plainToInstance(CoolerControlAllDeviceSettingsDTO, response.data as object)
        } catch (err: any) {
            this.logError(err)
            return new CoolerControlAllDeviceSettingsDTO()
        }
    }

    /**
     * Retrieves general CoolerControl settings for a specific Device from the daemon
     */
    async loadCCDeviceSettings(
        deviceUID: UID,
        device_name: string,
    ): Promise<CoolerControlDeviceSettingsDTO> {
        try {
            const response = await this.getClient().get(`/settings/devices/${deviceUID}`)
            this.logDaemonResponse(response, 'Load CC Settings for a device')
            return plainToInstance(CoolerControlDeviceSettingsDTO, response.data as object)
        } catch (err: any) {
            this.logError(err)
            return new CoolerControlDeviceSettingsDTO(deviceUID, device_name)
        }
    }

    /**
     * Persists and applies general CoolerControl settings for a specific device
     */
    async saveCCDeviceSettings(
        deviceUID: UID,
        ccDeviceSettings: CoolerControlDeviceSettingsDTO,
    ): Promise<boolean> {
        try {
            const response = await this.getClient().put(
                `/settings/devices/${deviceUID}`,
                instanceToPlain(ccDeviceSettings),
            )
            this.logDaemonResponse(response, 'Save CC Settings for a device')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Requests the Device Settings set for the specified Device UID.
     * Will return an empty array if there are no Settings for the device.
     * @param deviceUID
     */
    async loadDeviceSettings(deviceUID: UID): Promise<DeviceSettingsReadDTO> {
        try {
            const response = await this.getClient().get(`/devices/${deviceUID}/settings`)
            this.logDaemonResponse(response, 'Load Device Settings')
            return plainToInstance(DeviceSettingsReadDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            return new DeviceSettingsReadDTO()
        }
    }

    /**
     * Applies the specified device setting to the daemon.
     * @param deviceUID
     * @param channelName
     * @param setting
     */
    async saveDeviceSettingManual(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteManualDTO,
    ): Promise<boolean> {
        try {
            const response = await this.getClient().put(
                `/devices/${deviceUID}/settings/${channelName}/manual`,
                instanceToPlain(setting),
            )
            this.logDaemonResponse(response, 'Apply Device Setting Manual')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Applies the specified device setting to the daemon.
     * @param deviceUID
     * @param channelName
     * @param setting
     */
    async saveDeviceSettingProfile(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteProfileDTO,
    ): Promise<boolean> {
        try {
            const response = await this.getClient().put(
                `/devices/${deviceUID}/settings/${channelName}/profile`,
                instanceToPlain(setting),
            )
            this.logDaemonResponse(response, 'Apply Device Setting Profile')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Applies the specified device setting to the daemon.
     * @param deviceUID
     * @param channelName
     * @param setting
     */
    async saveDeviceSettingLcd(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteLcdDTO,
    ): Promise<boolean> {
        try {
            const response = await this.getClient().put(
                `/devices/${deviceUID}/settings/${channelName}/lcd`,
                instanceToPlain(setting),
            )
            this.logDaemonResponse(response, 'Apply Device Setting LCD')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    async getDeviceSettingLcdImage(
        deviceUID: UID,
        channelName: string,
    ): Promise<File | ErrorResponse> {
        try {
            const response = await this.getClient().get(
                `/devices/${deviceUID}/settings/${channelName}/lcd/images`,
                { responseType: 'arraybuffer' },
            )
            this.logDaemonResponse(response, 'Get LCD Image Files')
            const isGif = response.headers['content-type'] === 'image/gif'
            const fileExt = isGif ? 'gif' : 'png'
            const contentType = isGif ? 'image/gif' : 'image/png'
            return new File(
                [new Blob([response.data], { type: contentType })],
                `lcd_image.${fileExt}`,
                { type: contentType },
            )
        } catch (err: any) {
            this.logError(err)
            if (err.response != null && err.response.data != null) {
                // Needed as Axios does not support a dynamic response type (different response type for success & error)
                // see: https://github.com/axios/axios/issues/2434 (closed...)
                const decoder = new TextDecoder('utf-8')
                return plainToInstance(
                    ErrorResponse,
                    JSON.parse(decoder.decode(err.response.data)) as object,
                )
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    async saveDeviceSettingLcdImages(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteLcdDTO,
        files: Array<File>,
    ): Promise<undefined | ErrorResponse> {
        try {
            const response = await this.getClient().putForm(
                `/devices/${deviceUID}/settings/${channelName}/lcd/images`,
                {
                    mode: setting.mode,
                    brightness: setting.brightness,
                    orientation: setting.orientation,
                    'images[]': files,
                },
                {
                    timeout: this.daemonTimeoutExtended,
                    signal: AbortSignal.timeout(this.killClientTimeoutExtended),
                },
            )
            this.logDaemonResponse(response, 'Apply LCD Image Files')
            return undefined
        } catch (err: any) {
            this.logError(err)
            if (err.response) {
                return plainToInstance(ErrorResponse, err.response.data as object)
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    async processLcdImageFiles(
        deviceUID: UID,
        channelName: string,
        files: Array<File>,
    ): Promise<File | ErrorResponse> {
        try {
            const response = await this.getClient().postForm(
                `/devices/${deviceUID}/settings/${channelName}/lcd/images`,
                {
                    mode: 'image',
                    'images[]': files,
                },
                {
                    timeout: this.daemonTimeoutExtended,
                    signal: AbortSignal.timeout(this.killClientTimeoutExtended),
                    responseType: 'arraybuffer',
                },
            )
            this.logDaemonResponse(response, 'Process Image Files')
            const isGif = response.headers['content-type'] === 'image/gif'
            const fileExt = isGif ? 'gif' : 'png'
            const contentType = isGif ? 'image/gif' : 'image/png'
            return new File(
                [new Blob([response.data], { type: contentType })],
                `lcd_image.${fileExt}`,
                { type: contentType },
            )
        } catch (err: any) {
            this.logError(err)
            if (err.response != null && err.response.data != null) {
                // Needed as Axios does not support a dynamic response type (different response type for success & error)
                // see: https://github.com/axios/axios/issues/2434 (closed...)
                const decoder = new TextDecoder('utf-8')
                return plainToInstance(
                    ErrorResponse,
                    JSON.parse(decoder.decode(err.response.data)) as object,
                )
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    /**
     * Applies the specified device setting to the daemon.
     * @param deviceUID
     * @param channelName
     * @param setting
     */
    async saveDeviceSettingLighting(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWriteLightingDTO,
    ): Promise<boolean> {
        try {
            const response = await this.getClient().put(
                `/devices/${deviceUID}/settings/${channelName}/lighting`,
                instanceToPlain(setting),
            )
            this.logDaemonResponse(response, 'Apply Device Setting Lighting')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Applies the specified device setting to the daemon.
     * @param deviceUID
     * @param channelName
     * @param setting
     */
    async saveDeviceSettingPWM(
        deviceUID: UID,
        channelName: string,
        setting: DeviceSettingWritePWMModeDTO,
    ): Promise<boolean> {
        try {
            const response = await this.getClient().put(
                `/devices/${deviceUID}/settings/${channelName}/pwm`,
                instanceToPlain(setting),
            )
            this.logDaemonResponse(response, 'Apply Device Setting PWM Mode')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Applies the specified device setting to the daemon.
     * @param deviceUID
     * @param channelName
     */
    async saveDeviceSettingReset(deviceUID: UID, channelName: string): Promise<boolean> {
        try {
            const response = await this.getClient().put(
                `/devices/${deviceUID}/settings/${channelName}/reset`,
            )
            this.logDaemonResponse(response, 'Apply Device Setting RESET')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Retrieves the persisted Functions from the daemon.
     * @returns {FunctionsDTO}
     */
    async loadFunctions(): Promise<FunctionsDTO> {
        try {
            const response = await this.getClient().get('/functions')
            this.logDaemonResponse(response, 'Load Functions')
            return plainToInstance(FunctionsDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            return new FunctionsDTO()
        }
    }

    /**
     * Sends the Functions to the daemon for persistence of order ONLY.
     * @param functions {FunctionsDTO}
     */
    async saveFunctionsOrder(functions: FunctionsDTO): Promise<boolean> {
        try {
            const response = await this.getClient().post(
                '/functions/order',
                instanceToPlain(functions),
            )
            this.logDaemonResponse(response, 'Save Functions Order')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Sends the newly created Function to the daemon for persistence.
     * @param fun
     */
    async saveFunction(fun: Function): Promise<boolean> {
        try {
            const response = await this.getClient().post('/functions', instanceToPlain(fun))
            this.logDaemonResponse(response, 'Save Function')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Sends the newly updated Function to the daemon for persistence and updating of settings.
     * @param fun
     */
    async updateFunction(fun: Function): Promise<boolean> {
        try {
            const response = await this.getClient().put('/functions', instanceToPlain(fun))
            this.logDaemonResponse(response, 'Update Function')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Deletes the function from the daemon with the associated UID.
     * It also updates any settings that are affected.
     * @param functionsUID
     */
    async deleteFunction(functionsUID: UID): Promise<boolean> {
        try {
            const response = await this.getClient().delete(`/functions/${functionsUID}`)
            this.logDaemonResponse(response, 'Delete Function')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Retrieves the persisted Profiles from the daemon.
     * @returns {ProfilesDTO}
     */
    async loadProfiles(): Promise<ProfilesDTO> {
        try {
            const response = await this.getClient().get('/profiles')
            this.logDaemonResponse(response, 'Load Profiles')
            return plainToInstance(ProfilesDTO, response.data as object)
        } catch (err) {
            this.logError(err)
            return new ProfilesDTO()
        }
    }

    /**
     * Sends the Profiles to the daemon for persistence of order ONLY.
     * @param profiles {ProfilesDTO}
     */
    async saveProfilesOrder(profiles: ProfilesDTO): Promise<boolean> {
        try {
            const response = await this.getClient().post(
                '/profiles/order',
                instanceToPlain(profiles),
            )
            this.logDaemonResponse(response, 'Save Profiles Order')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Sends the newly created Profile to the daemon for persistence.
     * @param profile
     */
    async saveProfile(profile: Profile): Promise<boolean> {
        try {
            const response = await this.getClient().post('/profiles', instanceToPlain(profile))
            this.logDaemonResponse(response, 'Save Profile')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Sends the newly updated Profile to the daemon for persistence and updating of settings.
     * @param profile
     */
    async updateProfile(profile: Profile): Promise<boolean> {
        try {
            const response = await this.getClient().put('/profiles', instanceToPlain(profile))
            this.logDaemonResponse(response, 'Update Profile')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * Deletes the Profile from the daemon with the associated UID.
     * It also updates any settings that are affected.
     * @param profileUID
     */
    async deleteProfile(profileUID: UID): Promise<boolean> {
        try {
            const response = await this.getClient().delete(`/profiles/${profileUID}`)
            this.logDaemonResponse(response, 'Delete Profile')
            return true
        } catch (err) {
            this.logError(err)
            return false
        }
    }

    /**
     * The function `getCustomSensor` retrieves a custom sensor by its ID and returns either the custom
     * sensor object or an error response.
     * @param {string} customSensorID - The `customSensorID` parameter is a string that represents the ID
     * of a custom sensor. It is used to retrieve information about a specific custom sensor from the
     * server.
     * @returns The function `getCustomSensor` returns a Promise that resolves to either a `CustomSensor`
     * object or an `ErrorResponse` object.
     */
    async getCustomSensor(customSensorID: string): Promise<CustomSensor | ErrorResponse> {
        try {
            const response = await this.getClient().get(`/custom-sensors/${customSensorID}`)
            this.logDaemonResponse(response, 'Get Custom Sensor')
            return plainToInstance(CustomSensor, response.data as object)
        } catch (err: any) {
            this.logError(err)
            if (err.response) {
                return plainToInstance(ErrorResponse, err.response.data as object)
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    /**
     * The function `saveCustomSensor` saves a custom sensor by making a POST request to the
     * daemon and returns a boolean indicating whether the operation was successful or not.
     * @param {CustomSensor} sensor - The `sensor` parameter is an object of type `CustomSensor`. It
     * represents a custom sensor that needs to be saved.
     * @returns a Promise<boolean>.
     */
    async saveCustomSensor(sensor: CustomSensor): Promise<undefined | ErrorResponse> {
        try {
            const response = await this.getClient().post('/custom-sensors', instanceToPlain(sensor))
            this.logDaemonResponse(response, 'Save Custom Sensor')
            return
        } catch (err: any) {
            this.logError(err)
            if (err.response) {
                return plainToInstance(ErrorResponse, err.response.data as object)
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    /**
     * The function `updateCustomSensor` updates a custom sensor by making a PUT request to the daemon
     * and returns a boolean indicating whether the update was successful or not.
     * @param {CustomSensor} sensor - The `sensor` parameter is an object of type `CustomSensor`. It
     * represents the sensor that needs to be updated.
     * @returns a Promise<boolean>.
     */
    async updateCustomSensor(sensor: CustomSensor): Promise<undefined | ErrorResponse> {
        try {
            const response = await this.getClient().put('/custom-sensors', instanceToPlain(sensor))
            this.logDaemonResponse(response, 'Update Custom Sensor')
            return
        } catch (err: any) {
            this.logError(err)
            if (err.response) {
                return plainToInstance(ErrorResponse, err.response.data as object)
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    /**
     * The function `deleteCustomSensor` is an asynchronous function that deletes a custom sensor by its
     * ID and returns a boolean indicating whether the deletion was successful or not.
     * @param {String} customSensorID - The `customSensorID` parameter is a string that represents the ID
     * of the custom sensor that you want to delete.
     * @returns a Promise that resolves to a boolean value.
     */
    async deleteCustomSensor(customSensorID: string): Promise<undefined | ErrorResponse> {
        try {
            const response = await this.getClient().delete(`/custom-sensors/${customSensorID}`)
            this.logDaemonResponse(response, 'Delete Custom Sensor')
            return
        } catch (err: any) {
            this.logError(err)
            if (err.response) {
                return plainToInstance(ErrorResponse, err.response.data as object)
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    /**
     * Enables or Disables ThinkPad Fan Control.
     */
    async thinkPadFanControl(enable: boolean): Promise<undefined | ErrorResponse> {
        try {
            const response = await this.getClient().put('/thinkpad-fan-control', { enable: enable })
            this.logDaemonResponse(response, 'ThinkPad Fan Control')
            return undefined
        } catch (err: any) {
            this.logError(err)
            if (err.response) {
                return plainToInstance(ErrorResponse, err.response.data as object)
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    /**
     * Sets the AseTek device driver type.
     */
    async setAseTekDeviceType(
        deviceUID: UID,
        isLegacy690: boolean,
    ): Promise<undefined | ErrorResponse> {
        try {
            const response = await this.getClient().patch(`/devices/${deviceUID}/asetek690`, {
                is_legacy690: isLegacy690,
            })
            this.logDaemonResponse(response, 'AseTek Device Type')
            return undefined
        } catch (err: any) {
            this.logError(err)
            if (err.response) {
                return plainToInstance(ErrorResponse, err.response.data as object)
            } else {
                return new ErrorResponse('Unknown Cause')
            }
        }
    }

    /**
     * Sends a shutdown signal to the daemon. Systemd normally will restart the services automatically.
     */
    async shutdownDaemon(): Promise<boolean> {
        try {
            const response = await this.getClient().post('/shutdown', {})
            this.logDaemonResponse(response, 'Daemon Shutdown')
            return true
        } catch (err: any) {
            this.logError(err)
            return false
        }
    }
}
