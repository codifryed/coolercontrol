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
import { useConfirm } from 'primevue/useconfirm'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { ErrorResponse } from '@/models/ErrorResponse'
import type { Calibration, CalibrationStatus } from '@/models/Calibration'
import type { UID } from '@/models/Device'
import type { SpeedOptions } from '@/models/SpeedOptions'

/** Poll interval while a diagnosis is `in_progress` (milliseconds). */
const POLL_INTERVAL_MS = 1000

type ChannelKey = string

function channelKey(deviceUid: UID, channelName: string): ChannelKey {
    return `${deviceUid}::${channelName}`
}

/**
 * One queued prompt entry. The kind selects which i18n key the prompt
 * picks up; `channelName` is the user-facing display name pulled from
 * the settings store at queue time (so it survives later renames).
 *
 * `rpm_only_*` fires when the calibration's `was_rpm_only` flag is
 * true: the daemon backfilled (or, on clear, will reset) the channel's
 * status_history duty values, and the chart needs a remount to add or
 * remove the duty series. `duty_range_*` fires when the channel has a
 * non-default duty range (min_duty != 0 or max_duty != 100): the
 * manual-duty slider bounds need to refresh from `device.info`.
 */
type PendingPromptKind =
    | 'rpm_only_completed'
    | 'rpm_only_cleared'
    | 'duty_range_completed'
    | 'duty_range_cleared'

interface PendingPromptEntry {
    channelName: string
    kind: PendingPromptKind
}

export const useCalibrationStore = defineStore('calibration', () => {
    const deviceStore = useDeviceStore()
    const settingsStore = useSettingsStore()
    const confirm = useConfirm()
    const { t } = useI18n()

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

    /**
     * Queued UI-reload prompt entries. Non-reactive: the bundle
     * is consumed by `maybeFlushBundle` which empties it before
     * showing the dialog. A user calibrating several channels in
     * a row gets a single prompt once every diagnosis has settled.
     */
    const pendingPrompts = new Map<ChannelKey, PendingPromptEntry>()

    function getChannelDisplayName(uid: UID, channelName: string): string {
        return (
            settingsStore.allUIDeviceSettings.get(uid)?.sensorsAndChannels.get(channelName)?.name ??
            channelName
        )
    }

    function getSpeedOptions(uid: UID, channelName: string): SpeedOptions | undefined {
        for (const device of deviceStore.allDevices()) {
            if (device.uid === uid) {
                return device.info?.channels.get(channelName)?.speed_options
            }
        }
        return undefined
    }

    /**
     * Decide which prompt copy applies to a terminal calibration
     * transition. `was_rpm_only` takes precedence: when the daemon
     * backfilled (or, on clear, reset) duty values in status_history,
     * the chart needs a remount regardless of slider bounds. The
     * duty-range fallback only fires for channels whose effective
     * range is not the default 0..=100, so the prompt does not nag
     * users with normal hardware.
     */
    function classifyPromptKind(
        uid: UID,
        channelName: string,
        calibration: Calibration,
        eventKind: 'completed' | 'cleared',
    ): PendingPromptKind | undefined {
        if (calibration.was_rpm_only === true) {
            return eventKind === 'completed' ? 'rpm_only_completed' : 'rpm_only_cleared'
        }
        const opts = getSpeedOptions(uid, channelName)
        if (opts != null && (opts.min_duty !== 0 || opts.max_duty !== 100)) {
            return eventKind === 'completed' ? 'duty_range_completed' : 'duty_range_cleared'
        }
        return undefined
    }

    function anyInProgress(): boolean {
        for (const status of statuses.values()) {
            if (status.phase === 'in_progress') return true
        }
        return false
    }

    function queuePromptEntry(uid: UID, channelName: string, kind: PendingPromptKind): void {
        pendingPrompts.set(channelKey(uid, channelName), {
            channelName: getChannelDisplayName(uid, channelName),
            kind,
        })
        maybeFlushBundle()
    }

    function maybeFlushBundle(): void {
        if (pendingPrompts.size === 0) return
        if (anyInProgress()) return
        const entries = Array.from(pendingPrompts.values())
        pendingPrompts.clear()
        showBundlePrompt(entries)
    }

    function composePromptMessage(entries: PendingPromptEntry[]): string {
        const names = entries.map((e) => e.channelName)
        const list = names.join(', ')
        const single = names[0]
        const kinds = new Set(entries.map((e) => e.kind))
        if (kinds.size === 1) {
            const kind = entries[0].kind
            const suffix = entries.length === 1 ? 'Single' : 'Multi'
            const key = `components.channelExtensionSettings.calibration.reload_${kind}_${suffix.toLowerCase()}`
            return t(key, { channelName: single, channelList: list })
        }
        return t('components.channelExtensionSettings.calibration.reload_mixed_multi', {
            channelList: list,
        })
    }

    function showBundlePrompt(entries: PendingPromptEntry[]): void {
        confirm.require({
            header: t('components.channelExtensionSettings.calibration.reloadHeader'),
            message: composePromptMessage(entries),
            icon: 'pi pi-refresh',
            defaultFocus: 'reject',
            acceptLabel: t('components.channelExtensionSettings.calibration.reloadAccept'),
            rejectLabel: t('components.channelExtensionSettings.calibration.reloadReject'),
            accept: () => {
                deviceStore.reloadUI(true)
            },
        })
    }

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
        const key = channelKey(uid, channelName)
        const previous = statuses.get(key)
        statuses.set(key, status)
        // First transition into `completed`: queue a reload prompt if
        // the calibration backfilled history duties (was_rpm_only) or
        // the channel has a non-default duty range that the slider
        // bounds need to pick up. The bundle waits for every active
        // calibration to settle before firing a single dialog.
        if (status.phase === 'completed' && previous?.phase !== 'completed') {
            const kind = classifyPromptKind(uid, channelName, status.calibration, 'completed')
            if (kind !== undefined) {
                queuePromptEntry(uid, channelName, kind)
            } else {
                // No queue, but a stale bundle from prior calibrations
                // may now be eligible to flush since this in_progress
                // exited terminal.
                maybeFlushBundle()
            }
        } else if (status.phase === 'failed' && previous?.phase === 'in_progress') {
            // Failed transitions also exit `in_progress`; drain any
            // bundle stranded by prior completions.
            maybeFlushBundle()
        }
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
     *
     * Snapshots the calibration BEFORE deleting so the prompt
     * classifier can read `was_rpm_only` and the slider-bounds heuristic
     * can read `speed_options`. If either applies, the channel joins
     * the same reload-prompt bundle used by completion transitions.
     */
    async function deleteCalibration(
        uid: UID,
        channelName: string,
    ): Promise<boolean | ErrorResponse> {
        const key = channelKey(uid, channelName)
        const previous = statuses.get(key)
        const priorCalibration = previous?.phase === 'completed' ? previous.calibration : undefined
        const result = await deviceStore.daemonClient.deleteCalibration(uid, channelName)
        if (result === true || result === false) {
            stopPolling(uid, channelName)
            statuses.delete(key)
            if (priorCalibration !== undefined) {
                const kind = classifyPromptKind(uid, channelName, priorCalibration, 'cleared')
                if (kind !== undefined) {
                    queuePromptEntry(uid, channelName, kind)
                } else {
                    maybeFlushBundle()
                }
            } else {
                maybeFlushBundle()
            }
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

    /**
     * Replace the kick-boost and kick-duration override fields on the
     * stored calibration. Both fields are sent unconditionally; `null`
     * clears the corresponding override. On success the cached
     * `completed` status entry for the channel is refreshed with the
     * returned calibration so any subscribed UI sees the new values.
     */
    async function updateOverrides(
        uid: UID,
        channelName: string,
        overrides: {
            kick_boost_override: boolean | null
            kick_duration_override_ms: number | null
        },
    ): Promise<Calibration | undefined | ErrorResponse> {
        const result = await deviceStore.daemonClient.patchCalibrationOverrides(
            uid,
            channelName,
            overrides,
        )
        if (result && !(result instanceof ErrorResponse)) {
            const key = channelKey(uid, channelName)
            const existing = statuses.get(key)
            if (existing?.phase === 'completed') {
                statuses.set(key, { ...existing, calibration: result })
            }
        }
        return result
    }

    /**
     * Pull every persisted calibration in one request and prime the
     * `statuses` map with synthesised `completed` entries. Called once
     * at app load so the tree menu can render a "calibrated" pill for
     * every applicable channel without a request per channel.
     *
     * Channels that are mid-sweep (`in_progress` already in the map)
     * are left alone so an active diagnosis is not overwritten.
     */
    async function refreshAllStatuses(): Promise<void> {
        const entries = await deviceStore.daemonClient.getAllCalibrations()
        for (const entry of entries) {
            const key = channelKey(entry.device_uid, entry.channel_name)
            const existing = statuses.get(key)
            if (existing?.phase === 'in_progress') continue
            statuses.set(key, {
                phase: 'completed',
                device_uid: entry.device_uid,
                channel_name: entry.channel_name,
                completed_at: entry.calibration.timestamp,
                calibration: entry.calibration,
            })
        }
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
        updateOverrides,
        refreshAllStatuses,
    }
})
