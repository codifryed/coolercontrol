/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

/**
 * Calibration state + API client glue.
 *
 * Calibration uses a polling architecture on the wire (no SSE). This
 * store owns:
 *
 * - A reactive Map of channel-key -> `CalibrationStatus`, updated by
 *   periodic polls of `GET .../calibration/status`.
 * - A non-reactive Map of channel-key -> `setInterval` handle so the
 *   poller can be torn down on terminal status transitions or store
 *   reset.
 * - Thin wrappers around the four lifecycle endpoints (start, cancel,
 *   get, delete) that piggyback on `DeviceStore`'s `daemonClient`.
 *
 * UI components subscribe to `statusFor(uid, channel)` to render
 * progress bars and final-result UI. They call `startCalibration` to
 * kick a diagnosis off; the store automatically begins polling once
 * the start request succeeds, and stops once the status reaches a
 * terminal phase.
 */

import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore'
import { ErrorResponse } from '@/models/ErrorResponse'
import type { Calibration, CalibrationStatus } from '@/models/Calibration'
import type { UID } from '@/models/Device'

/** Poll interval while a diagnosis is `in_progress` (milliseconds). */
const POLL_INTERVAL_MS = 1000

type ChannelKey = string

function channelKey(deviceUid: UID, channelName: string): ChannelKey {
    return `${deviceUid}::${channelName}`
}

export const useCalibrationStore = defineStore('calibration', () => {
    const deviceStore = useDeviceStore()

    /**
     * Latest known status per channel. Updated by the polling loop;
     * Vue components reading `statusFor(uid, channel)` will react on
     * each new sample.
     */
    const statuses = reactive(new Map<ChannelKey, CalibrationStatus>())

    /**
     * Active poller handles. Non-reactive: components do not need
     * to observe whether a poller is alive (they react to the
     * `phase` field of the status itself).
     */
    const pollers = new Map<ChannelKey, ReturnType<typeof setInterval>>()

    function statusFor(uid: UID, channelName: string): CalibrationStatus | undefined {
        return statuses.get(channelKey(uid, channelName))
    }

    function isPolling(uid: UID, channelName: string): boolean {
        return pollers.has(channelKey(uid, channelName))
    }

    async function refreshStatus(uid: UID, channelName: string): Promise<void> {
        const status = await deviceStore.daemonClient.getCalibrationStatus(uid, channelName)
        if (status === undefined) {
            // Transport error: keep the last-known entry. The daemon
            // returns a `NotStarted` payload (HTTP 200) for the "no
            // status yet" case, so `undefined` here only signals a
            // network failure.
            return
        }
        statuses.set(channelKey(uid, channelName), status)
    }

    /**
     * Refresh the status once, and start the polling loop if the
     * channel turns out to be mid-diagnosis. Use this on component
     * mount to recover the running state after a page reload.
     */
    async function ensurePolling(uid: UID, channelName: string): Promise<void> {
        await refreshStatus(uid, channelName)
        const current = statuses.get(channelKey(uid, channelName))
        if (current?.phase === 'in_progress' && !isPolling(uid, channelName)) {
            startPolling(uid, channelName)
        }
    }

    function startPolling(uid: UID, channelName: string): void {
        const key = channelKey(uid, channelName)
        if (pollers.has(key)) {
            return
        }
        // Kick an immediate refresh so the UI sees state without
        // waiting one full interval; failures are silent.
        refreshStatus(uid, channelName).catch(() => {})
        const intervalId = setInterval(async () => {
            await refreshStatus(uid, channelName)
            const current = statuses.get(key)
            if (!current || current.phase !== 'in_progress') {
                stopPolling(uid, channelName)
            }
        }, POLL_INTERVAL_MS)
        pollers.set(key, intervalId)
    }

    function stopPolling(uid: UID, channelName: string): void {
        const key = channelKey(uid, channelName)
        const intervalId = pollers.get(key)
        if (intervalId !== undefined) {
            clearInterval(intervalId)
            pollers.delete(key)
        }
    }

    /**
     * Start a diagnosis on the channel. On success, the store begins
     * polling the channel's status at 1 Hz. The poller stops itself
     * when the status reaches a terminal phase. The returned value
     * mirrors `DaemonClient.startCalibration` so callers can surface
     * 409 conflicts or other errors.
     */
    async function startCalibration(
        uid: UID,
        channelName: string,
    ): Promise<boolean | ErrorResponse> {
        const result = await deviceStore.daemonClient.startCalibration(uid, channelName)
        if (result === true) {
            startPolling(uid, channelName)
        }
        return result
    }

    /**
     * Cancel an in-flight diagnosis. The poller is left running so
     * the UI observes the subsequent `Failed { reason: 'user_cancelled' }`
     * transition before the poller naturally stops.
     */
    async function cancelCalibration(
        uid: UID,
        channelName: string,
    ): Promise<boolean | ErrorResponse> {
        return deviceStore.daemonClient.cancelCalibration(uid, channelName)
    }

    /**
     * Delete the persisted calibration. Clears the cached status and
     * stops any in-flight poller so the UI immediately reflects the
     * uncalibrated state.
     */
    async function deleteCalibration(
        uid: UID,
        channelName: string,
    ): Promise<boolean | ErrorResponse> {
        const result = await deviceStore.daemonClient.deleteCalibration(uid, channelName)
        if (result === true || result === false) {
            stopPolling(uid, channelName)
            statuses.delete(channelKey(uid, channelName))
        }
        return result
    }

    /**
     * Fetch the persisted calibration without touching the status
     * map. Used by UI that wants to inspect calibration data after
     * a diagnosis succeeded but doesn't have it cached locally.
     */
    async function getStored(uid: UID, channelName: string): Promise<Calibration | undefined> {
        return deviceStore.daemonClient.getCalibration(uid, channelName)
    }

    return {
        statusFor,
        isPolling,
        refreshStatus,
        ensurePolling,
        startPolling,
        stopPolling,
        startCalibration,
        cancelCalibration,
        deleteCalibration,
        getStored,
    }
})
