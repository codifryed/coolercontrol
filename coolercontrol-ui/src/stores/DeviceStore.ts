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

import {defineStore} from 'pinia'
import {Device, DeviceType, type UID} from "@/models/Device"
import DaemonClient from "@/stores/DaemonClient"
import {ChannelInfo} from "@/models/ChannelInfo"
import {DeviceResponseDTO} from "@/stores/DataTransferModels"
import {ref, shallowRef, triggerRef} from "vue"
import {useLayout} from "@/layout/composables/layout"
import {useConfirm} from "primevue/useconfirm"
import {useToast} from "primevue/usetoast";
import {ErrorResponse} from "@/models/ErrorResponse";

/**
 * This is similar to the model_view in the old GUI, where it held global state for all the various hooks and accesses
 */
export interface ChannelValues {
  temp?: string
  rpm?: string
  duty?: string
}

export const useDeviceStore =
    defineStore('device', () => {

      // Internal properties that we don't want to be reactive (overhead) ------------------------------------------------
      const devices = new Map<UID, Device>()
      const daemonClient = new DaemonClient()
      const confirm = useConfirm()
      const toast = useToast()
      // One benefit of having this set pretty low (2000) is that it refreshes all the statuses every so often,
      //   which helps when the daemon has recently restarted. Otherwise, one needs a full refresh.
      //   The downside is that there is occasion more disruption in the UI due to the work needed to reload all statuses.
      const reloadAllStatusesThreshold: number = 2_000
      // -----------------------------------------------------------------------------------------------------------------

      // Reactive properties ------------------------------------------------

      const currentDeviceStatus = shallowRef(new Map<UID, Map<string, ChannelValues>>())
      const isThinkPad = ref(false)
      const fontScale = ref(useLayout().layoutConfig.scale.value)

      // Getters ---------------------------------------------------------------------------------------------------------
      function allDevices(): IterableIterator<Device> {
        return devices.values()
      }

      function sleep(ms: number): Promise<number> {
        return new Promise(r => setTimeout(r, ms))
      }

      async function waitAndReload(secs: number = 3): Promise<void> {
        await sleep(secs * 1000)
        // When accessing the UI directly from the daemon, we need to refresh on the base URL.
        window.location.replace('/')
      }

      function toTitleCase(str: string): string {
        return str.replace(
            /\w\S*/g,
            (txt: string) => txt.charAt(0).toUpperCase() + txt.substring(1).toLowerCase()
        )
      }

      function limitStringLength(str: string, limit: number): string {
        return str.substring(0, limit)
      }

      function sanitizeString(str: string, lengthLimit: number = 22): string {
        return limitStringLength(str.trim(), lengthLimit)
      }

      function round(value: number, precision: number = 0): number {
        const multiplier = Math.pow(10, precision)
        return Math.round(value * multiplier) / multiplier
      }

      function getREMSize(rem: number): number {
        const _ = fontScale.value // used to reactively recalculate the following values:
        const fontSize = window.getComputedStyle(document.querySelector('html')!).fontSize
        return parseFloat(fontSize) * rem
      }

      function isTauriApp(): boolean {
        return '__TAURI__' in window
      }

      // Private methods ------------------------------------------------
      /**
       * Sorts the devices in the DeviceResponseDTO by first type, and then by typeIndex
       */
      function sortDevices(dto: DeviceResponseDTO): void {
        dto.devices.sort((a, b) => {
          const aTypeOrdinal = Object.values(DeviceType).indexOf(a.type)
          const bTypeOrdinal = Object.values(DeviceType).indexOf(b.type)
          if (aTypeOrdinal > bTypeOrdinal) {
            return 1
          } else if (aTypeOrdinal < bTypeOrdinal) {
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
          device.info.channels = new Map<string, ChannelInfo>(
              [...device.info.channels.entries()].sort(
                  ([c1name, _c1i], [c2name, _c2i]) =>
                      c1name.localeCompare(c2name)
              )
          )
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
        sortDevices(dto)
        for (const device of dto.devices) {
          if (device.type === DeviceType.COMPOSITE ) {
              continue // the composite device is DEPRECATED and will be remove in a future release
          }
          if (device.info?.thinkpad_fan_control != null) {
            isThinkPad.value = true
          }
          if (device.lc_info?.unknown_asetek) {
            confirm.require({
              group: 'AseTek690',
              message: `${device.type_index}`,
              header: 'Unknown Device Detected',
              icon: 'pi pi-exclamation-triangle',
              acceptLabel: 'Yes, It\'s a legacy Kraken Device',
              rejectLabel: 'No, It\'s a EVGA CLC Device',
              accept: async () => {
                console.debug(`Setting device ${device.uid} as a Legacy 690`)
                await handleAseTekResponse(device.uid, true)
              },
              reject: async () => {
                console.debug(`Setting device ${device.uid} as a EVGA CLC`)
                await handleAseTekResponse(device.uid, false)
              }
            })
          }
          sortChannels(device);
          devices.set(device.uid, device);
        }
        await loadCompleteStatusHistory()
        console.debug('Initialized with devices:')
        console.debug(devices)
        return true
      }

      async function handleAseTekResponse(deviceUID: UID, isLegacy690: boolean): Promise<void> {
        const response = await daemonClient.setAseTekDeviceType(deviceUID, isLegacy690)
        if (response instanceof ErrorResponse) {
          toast.add({severity: 'error', summary: 'Error', detail: response.error + ' - Process interrupted.', life: 4000})
          return
        }
        const msg = isLegacy690 ? 'Device Model type successfully set. Restart in progress.' : 'Device Model type successfully set.'
        toast.add({severity: 'success', summary: 'Success', detail: msg, life: 3000})
        if (isLegacy690) {
          await daemonClient.shutdownDaemon();
          await waitAndReload()
        }
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
        updateRecentDeviceStatus()
      }


      /**
       * Requests the most recent status for all devices and adds it to the current status array.
       * @return boolean true if only the most recent status was updated. False if all statuses were updated.
       */
      async function updateStatus(): Promise<boolean> {
        let onlyLatestStatus: boolean = true
        let timeDiffMillis: number = 0
        const dto = await daemonClient.recentStatus()
        if (dto.devices.length === 0 || dto.devices[0].status_history.length === 0) {
          return onlyLatestStatus // we can't update anything without data, which happens on daemon restart & resuming from sleep
        }
        if (devices.size > 0) {
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
              statuses.shift()
            }
          }
          updateRecentDeviceStatus()
        } else {
          console.debug(`[${new Date().toUTCString()}]:\nDevice Statuses are out of sync by ${new Intl.NumberFormat().format(timeDiffMillis)}ms, reloading all.`)
          await loadCompleteStatusHistory()
        }
        return onlyLatestStatus
      }

      function updateRecentDeviceStatus(): void {
        for (const [uid, device] of devices) {
          if (!currentDeviceStatus.value.has(uid)) {
            currentDeviceStatus.value.set(uid, new Map<string, ChannelValues>())
          }
          let deviceStatuses = currentDeviceStatus.value.get(uid)!
          for (const temp of device.status.temps) {
            if (deviceStatuses.has(temp.name)) {
              deviceStatuses.get(temp.name)!.temp = temp.temp.toFixed(1)
            } else {
              deviceStatuses.set(temp.name, {temp: temp.temp.toFixed(1)})
            }
          }
          for (const channel of device.status.channels) { // This gives us both "load" and "speed" channels
            if (deviceStatuses.has(channel.name)) {
              deviceStatuses.get(channel.name)!.duty = channel.duty?.toFixed(0)
              deviceStatuses.get(channel.name)!.rpm = channel.rpm?.toFixed(0)
            } else {
              deviceStatuses.set(channel.name, {
                duty: channel.duty?.toFixed(0),
                rpm: channel.rpm?.toFixed(0)
              })
            }
          }
        }
        triggerRef(currentDeviceStatus)
      }

      console.debug(`Device Store created`)
      return {
        daemonClient,
        allDevices,
        sleep,
        waitAndReload,
        toTitleCase,
        initializeDevices,
        fontScale,
        loadCompleteStatusHistory,
        updateStatus,
        currentDeviceStatus,
        round,
        sanitizeString,
        getREMSize,
        isTauriApp,
        isThinkPad,
      }
    })