import {ref} from 'vue'
import {defineStore} from 'pinia'
import {Device} from "@/models/Device";
import DaemonClient from "@/stores/DaemonClient";
import {ChannelInfo} from "@/models/ChannelInfo";
import {DeviceResponseDTO} from "@/stores/DataTransferModels";

/**
 * This is similar to the model_view in the old GUI, where it held global state for all the various hooks and accesses
 */
export const useDeviceStore = defineStore('device', () => {

    // Internal properties that we don't want to be reactive (overhead) ------------------------------------------------
    const devices = new Map<string, Device>()
    const daemonClient = new DaemonClient()
    const reloadAllStatusesThreshold: number = 2_000
    // const settingsObserver: any = null  // todo: this should be reactive to a Settings Store
    // const compositeTempsEnabled: boolean = false // todo: get from settings
    // const hwmonTempsEnabled: boolean = false // todo: get from settings
    // const hwmonFilterEnabled: boolean = false // todo: get from settings
    // const cpuCoreTempsEnabled: boolean = false // todo: get from settings
    // const excludedChannelNames: Map<string, string[]> = new Map<string, string[]>()
    // -----------------------------------------------------------------------------------------------------------------

    // Getters ---------------------------------------------------------------------------------------------------------
    // const allDevices = computed(() => devices.values()) // computed caches
    function allDevices(): IterableIterator<Device> {
        return devices.values()
    }

    // Private methods ------------------------------------------------
    /**
     * Sorts the devices in the DeviceResponseDTO by first type, and then by typeIndex
     */
    function sortDevices(dto: DeviceResponseDTO): void {
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
     */
    function sortChannels(device: Device): void {
        if (device.info?.channels) {
            device.info.channels = new Map<string, ChannelInfo>([...device.info.channels.entries()].sort())
        }
    }

    // Actions -----------------------------------------------------------------------
    async function initializeDevices(): Promise<boolean> {
        console.info("Initializing Devices")
        const handshakeSuccessful = await daemonClient.handshake()
        if (!handshakeSuccessful) {
            return false
        }
        const dto = await daemonClient.requestDevices()
        if (dto.devices.length === 0) {
            console.warn("There are no available devices!")
        }
        sortDevices(dto);
        for (const device of dto.devices) {
            // todo: check if unknownAsetek and do appropriate handling (restart)
            sortChannels(device);
            // todo: handle thinkpadFanControl
            devices.set(device.uid, device)
        }
        await loadCompleteStatusHistory()
        // todo: filter devices
        // todo: update device colors (should be interesting)
        // todo:use d3 color lib?
        console.debug('Initialized with devices:')
        console.debug(devices)
        return true
    }

    /**
     * requests and loads all the statuses for each device.
     */
    async function loadCompleteStatusHistory(): Promise<void> {
        const allStatusesDto = await daemonClient.completeStatusHistory()
        for (const dtoDevice of allStatusesDto.devices) {
            // not all device UIDs are present locally (composite can be ignored for example)
            if (devices.has(dtoDevice.uid)) {
                const statuses = devices.get(dtoDevice.uid)!.status_history
                statuses.length = 0 // clear array if this is a re-sync
                statuses.push(...dtoDevice.status_history) // shallow copy
            }
        }
    }


    /**
     * Requests the most recent status for all devices and adds it to the current status array.
     * @return boolean true if only the most recent status was updated. False if all statuses were updated.
     */
    async function updateStatus(): Promise<boolean> {
        let onlyLatestStatus: boolean = true
        let timeDiffMillis: number = 0;
        const dto = await daemonClient.recentStatus()
        if (dto.devices.length > 0 && devices.size > 0) {
            const device: Device = devices.values().next().value
            timeDiffMillis = Math.abs(
                new Date(device.status.timestamp).getTime()
                - new Date(dto.devices[0].status_history[0].timestamp).getTime()
            )
            if (timeDiffMillis > reloadAllStatusesThreshold) {
                onlyLatestStatus = false
            }
        }

        if (onlyLatestStatus) {
            for (const dtoDevice of dto.devices) {
                // not all device UIDs are present locally (composite can be ignored for example)
                if (devices.has(dtoDevice.uid)) {
                    const statuses = devices.get(dtoDevice.uid)!.status_history
                    statuses.push(...dtoDevice.status_history)
                    // todo: verify that the new status is indeed "new" / timestamp != last timestamp:
                    //  AND that the size of the array hasn't reached it's theoretical maximum (1860)
                    statuses.shift()
                }
            }
        } else {
            console.debug(`[${new Date().toUTCString()}]:\nDevice Statuses are out of sync by ${new Intl.NumberFormat().format(timeDiffMillis)}ms, reloading all.`)
            await loadCompleteStatusHistory()
        }
        return onlyLatestStatus
    }

    console.info(`Device Store created`)
    return {allDevices, initializeDevices, loadCompleteStatusHistory, updateStatus}
})