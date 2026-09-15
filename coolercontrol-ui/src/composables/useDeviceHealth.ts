// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { useI18n } from 'vue-i18n'
import type { UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'

// The daemon reports a wedged device as both unreachable and failsafe, since its channels go
// stale too. Every surface resolves that the same way: the device state wins, because it is the
// cause, and "not responding" is what the user can act on.
export function useDeviceHealth() {
    const { t } = useI18n()
    const settingsStore = useSettingsStore()

    const isDeviceUnreachable = (deviceUID: UID): boolean =>
        settingsStore.healthUnreachable.some((ref) => ref.device_uid === deviceUID)

    const isDeviceUnhealthy = (deviceUID: UID): boolean =>
        isDeviceUnreachable(deviceUID) ||
        settingsStore.healthFailsafe.some((ref) => ref.device_uid === deviceUID)

    const unreachableText = (): string =>
        `${t('views.appInfo.deviceUnreachable')}: ${t('views.appInfo.deviceUnreachableDetail')}`

    const failsafeText = (reason?: string): string => {
        const base = t('views.appInfo.failsafeActive')
        return reason ? `${base}: ${reason}` : base
    }

    const healthTooltip = (deviceUID: UID, channelName?: string): string => {
        if (isDeviceUnreachable(deviceUID)) return unreachableText()
        const ref = settingsStore.healthFailsafe.find(
            (entry) =>
                entry.device_uid === deviceUID &&
                (channelName == null || entry.name === channelName),
        )
        return failsafeText(ref?.reason)
    }

    return {
        isDeviceUnreachable,
        isDeviceUnhealthy,
        unreachableText,
        failsafeText,
        healthTooltip,
    }
}
