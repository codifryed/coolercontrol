import {markRaw, ref} from 'vue'
import {defineStore} from 'pinia'
import {DeviceService} from "@/service/DeviceService";

/**
 * This is similar to the model_view in the old GUI, where it held global state for all the various hooks and accesses
 */
export const useDeviceStore = defineStore('device', () => {
    const service = markRaw(new DeviceService())
    const fullStatusUpdate = ref(false)

    // todo: could have a function to updateStatuses and have a variable to maintain the last sync state, whether it was for all statuses or only the most recent

    return {service, fullStatusUpdate}
})