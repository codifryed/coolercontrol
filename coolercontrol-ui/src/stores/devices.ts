import {ref, computed} from 'vue'
import {defineStore} from 'pinia'
import {Device} from "@/models/Device";
import {DaemonService} from "@/service/DaemonService";

/**
 * This is similar to the model_view in the old GUI, where it held global state for all the various hooks and accesses
 */
export const useDeviceStore = defineStore('device', () => {
    let devices_array: Device[] = []
    const devices = ref(devices_array)
    const deviceService = ref(new DaemonService())

    function addDevices(additional_devices: Device[]): void {
        additional_devices.forEach(
                (dev: Device) => devices.value.push(dev)
        )
    }

    return {devices, deviceService, addDevices}
})