// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Device hardware-settings mutations (confirm + save + toast + daemon
// restart), ported from the old tree's extension-settings popover and the
// settings Device tab. Lives here so the shell's DevicePage stays free of
// direct PrimeVue imports (dependency discipline).

import { useConfirm } from '@/shell/confirm'
import { useToast } from '@/shell/toast'
import { useI18n } from 'vue-i18n'
import { mdiAlertOutline } from '@mdi/js'
import type { UID } from '@/models/Device.ts'
import { CCChannelSettings, CoolerControlDeviceSettingsDTO } from '@/models/CCSettings.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'

// 'cancelled' is the user backing out of the restart confirm, which must not be
// treated as a failure: only 'failed' means the daemon refused the change.
export type SensorChangeOutcome = 'saved' | 'failed' | 'cancelled'

export function useDeviceActions() {
    const confirm = useConfirm()
    const toast = useToast()
    const { t } = useI18n()
    const deviceStore = useDeviceStore()
    const settingsStore = useSettingsStore()

    const saveAndRestart = async (ccSetting: CoolerControlDeviceSettingsDTO): Promise<boolean> => {
        const result = await deviceStore.daemonClient.saveCCDeviceSettings(ccSetting.uid, ccSetting)
        // give the system a moment to make sure the setting has been saved
        await deviceStore.sleep(50)
        if (result === true) {
            toast.add({
                severity: 'success',
                summary: t('layout.settings.success'),
                detail: t('layout.settings.successDetail'),
                life: 6000,
            })
            await deviceStore.daemonClient.shutdownDaemon()
            await deviceStore.waitAndReload()
            return true
        }
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: result.error || t('layout.settings.devices.unknownError'),
            life: 6000,
        })
        return false
    }

    const confirmRestart = (accept: () => void | Promise<void>, reject?: () => void): void => {
        confirm.require({
            header: t('layout.settings.restartHeader'),
            message: t('layout.settings.applySettingAndRestart'),
            icon: mdiAlertOutline,
            defaultFocus: 'accept',
            acceptLabel: t('common.yes'),
            rejectLabel: t('common.no'),
            accept,
            reject,
        })
    }

    // A rejected save has to put the store back, as switchToHwmon already does. The
    // caller compares its control against this value to decide what is pending, so
    // leaving the refused value in place both strands the control in its new position
    // and lets the next successful save from any surface persist the change silently.
    const setDirectAccess = (deviceUID: UID, value: boolean, onReject: () => void): void => {
        confirmRestart(async () => {
            const ccSetting = settingsStore.ccDeviceSettings.get(deviceUID)!
            const previous = ccSetting.extensions.direct_access
            ccSetting.extensions.direct_access = value
            if (!(await saveAndRestart(ccSetting))) {
                ccSetting.extensions.direct_access = previous
                onReject()
            }
        }, onReject)
    }

    const setDelayMillis = (deviceUID: UID, value: number, onReject: () => void): void => {
        confirmRestart(async () => {
            const ccSetting = settingsStore.ccDeviceSettings.get(deviceUID)!
            const previous = ccSetting.extensions.delay_millis
            ccSetting.extensions.delay_millis = value
            if (!(await saveAndRestart(ccSetting))) {
                ccSetting.extensions.delay_millis = previous
                onReject()
            }
        }, onReject)
    }

    const setPerThreadFreq = (deviceUID: UID, value: boolean, onReject: () => void): void => {
        confirmRestart(async () => {
            const ccSetting = settingsStore.ccDeviceSettings.get(deviceUID)!
            const previous = ccSetting.extensions.per_thread_freq
            ccSetting.extensions.per_thread_freq = value
            if (!(await saveAndRestart(ccSetting))) {
                ccSetting.extensions.per_thread_freq = previous
                onReject()
            }
        }, onReject)
    }

    // Disables the liquidctl device so the kernel hwmon driver takes over.
    const switchToHwmon = (deviceUID: UID, onReject: () => void): void => {
        confirm.require({
            header: t('components.deviceExtensionSettings.disableDevice'),
            message: t('components.deviceExtensionSettings.disableInfo'),
            icon: mdiAlertOutline,
            defaultFocus: 'accept',
            acceptLabel: t('common.yes'),
            rejectLabel: t('common.no'),
            accept: async () => {
                const ccSetting = settingsStore.ccDeviceSettings.get(deviceUID)!
                ccSetting.disable = true
                const success = await saveAndRestart(ccSetting)
                if (!success) onReject()
            },
            reject: onReject,
        })
    }

    const enableAmdOverdrive = async (): Promise<void> => {
        const result = await deviceStore.daemonClient.amdGpuOverdriveEnable()
        if (result instanceof ErrorResponse) {
            toast.add({
                severity: 'error',
                summary: t('common.error'),
                detail: result.error ?? t('layout.settings.devices.unknownError'),
                life: 6000,
            })
        } else {
            toast.add({
                severity: 'success',
                summary: t('components.deviceExtensionSettings.overdriveSuccess'),
                detail: result,
                life: 10000,
            })
        }
    }

    const applyThinkPadFanControl = (enable: boolean): void => {
        settingsStore.applyThinkPadFanControl(enable)
    }

    // Writes one device's enable/disable state into its CC settings in place,
    // returning the settings to save (or null if the device has no settings).
    // The daemon stamps device and channel detection memos itself, so
    // client-supplied names are ignored (write-deprecated).
    const writeDeviceEnableState = (
        deviceUID: UID,
        deviceEnabled: boolean,
        channelStates: Map<string, boolean>,
    ): CoolerControlDeviceSettingsDTO | null => {
        const ccSetting = settingsStore.ccDeviceSettings.get(deviceUID)
        if (ccSetting == null) {
            console.error(`CCDeviceSetting not found for this device: ${deviceUID}`)
            return null
        }
        ccSetting.disable = !deviceEnabled
        if (deviceEnabled) {
            for (const [channelName, enabled] of channelStates) {
                let channelSettings = ccSetting.channel_settings.get(channelName)
                if (channelSettings == null) {
                    // only already-applied channel changes exist in
                    // channel_settings; enabled channels with no settings are
                    // not persisted
                    if (enabled) continue
                    channelSettings = new CCChannelSettings()
                    ccSetting.channel_settings.set(channelName, channelSettings)
                }
                channelSettings.disabled = !enabled
            }
        }
        return ccSetting
    }

    // What one device's enable state looked like before `writeDeviceEnableState` touched
    // it, so an edit the daemon refuses can be taken back out of the store.
    type DeviceEnableSnapshot = {
        deviceUID: UID
        disable: boolean
        channelDisabled: Map<string, boolean>
    }

    const snapshotDeviceEnableState = (deviceUID: UID): DeviceEnableSnapshot | null => {
        const ccSetting = settingsStore.ccDeviceSettings.get(deviceUID)
        if (ccSetting == null) return null
        const channelDisabled = new Map<string, boolean>()
        for (const [channelName, channelSettings] of ccSetting.channel_settings) {
            channelDisabled.set(channelName, channelSettings.disabled)
        }
        return { deviceUID, disable: ccSetting.disable, channelDisabled }
    }

    const restoreDeviceEnableState = (snapshot: DeviceEnableSnapshot): void => {
        const ccSetting = settingsStore.ccDeviceSettings.get(snapshot.deviceUID)
        if (ccSetting == null) return
        ccSetting.disable = snapshot.disable
        for (const channelName of [...ccSetting.channel_settings.keys()]) {
            // Entries writeDeviceEnableState created for this attempt were not persisted
            // before, so they go rather than being reset.
            if (!snapshot.channelDisabled.has(channelName)) {
                ccSetting.channel_settings.delete(channelName)
            }
        }
        for (const [channelName, disabled] of snapshot.channelDisabled) {
            const channelSettings = ccSetting.channel_settings.get(channelName)
            if (channelSettings != null) channelSettings.disabled = disabled
        }
    }

    // Batch sensor/device enable/disable across any number of devices,
    // preserving the old settings Device tab semantics: one confirm, then each
    // changed device is saved, then a single daemon restart on success. On
    // daemon rejection the error is shown verbatim (dependency messages) and
    // the daemon is NOT restarted.
    const applySensorChangesBatch = (
        edits: Map<UID, { deviceEnabled: boolean; channelStates: Map<string, boolean> }>,
        onDone: (outcome: SensorChangeOutcome) => void,
    ): void => {
        confirm.require({
            message: t('layout.settings.devices.toggleRequiresRestart'),
            header: t('layout.settings.devices.enableDevices'),
            icon: mdiAlertOutline,
            accept: async () => {
                const settingsToSave: CoolerControlDeviceSettingsDTO[] = []
                // Snapshots stay in step with settingsToSave, so a failure at index i
                // knows exactly which devices were never persisted.
                const snapshots: DeviceEnableSnapshot[] = []
                for (const [deviceUID, edit] of edits) {
                    const snapshot = snapshotDeviceEnableState(deviceUID)
                    const ccSetting = writeDeviceEnableState(
                        deviceUID,
                        edit.deviceEnabled,
                        edit.channelStates,
                    )
                    if (ccSetting != null && snapshot != null) {
                        settingsToSave.push(ccSetting)
                        snapshots.push(snapshot)
                    }
                }
                if (settingsToSave.length === 0) {
                    onDone('failed')
                    return
                }
                for (const [index, ccSetting] of settingsToSave.entries()) {
                    const result = await deviceStore.daemonClient.saveCCDeviceSettings(
                        ccSetting.uid,
                        ccSetting,
                    )
                    if (result instanceof ErrorResponse) {
                        // Only what was not persisted goes back: the devices saved before
                        // this one are live in the daemon and the store must keep matching
                        // them. Without this the caller re-seeds from a store holding
                        // edits the daemon refused, showing them as current and computing
                        // no pending changes.
                        for (const stale of snapshots.slice(index)) {
                            restoreDeviceEnableState(stale)
                        }
                        toast.add({
                            severity: 'error',
                            summary: t('common.error'),
                            detail: result.error || t('layout.settings.devices.unknownError'),
                            life: 0,
                        })
                        onDone('failed')
                        return
                    }
                }
                onDone('saved')
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload()
            },
            // Backing out of the confirm is not a failure: the pending edits are
            // still the user's, so callers must be able to leave them alone.
            reject: () => onDone('cancelled'),
        })
    }

    const applySensorChanges = (
        deviceUID: UID,
        deviceEnabled: boolean,
        channelStates: Map<string, boolean>,
        onDone: (outcome: SensorChangeOutcome) => void,
    ): void => {
        applySensorChangesBatch(new Map([[deviceUID, { deviceEnabled, channelStates }]]), onDone)
    }

    return {
        setDirectAccess,
        setDelayMillis,
        setPerThreadFreq,
        switchToHwmon,
        enableAmdOverdrive,
        applyThinkPadFanControl,
        applySensorChanges,
        applySensorChangesBatch,
    }
}
