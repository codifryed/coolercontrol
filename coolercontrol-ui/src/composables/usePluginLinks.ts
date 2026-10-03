// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useDialog } from '@/shell/dialog'
import { useToast } from '@/shell/toast'
import { validatePluginLinkForUi } from '@/composables/pluginLinkValidation.ts'
import PluginLinkDialog from '@/shell/plugins/PluginLinkDialog.vue'

// One confirmation at a time, across every plugin surface.
let isAsking = false

/**
 * How long a plugin's requests go unanswered after a prompt closes or a link is blocked. It
 * outlasts the few seconds a click keeps the window activated.
 */
export const PLUGIN_LINK_COOLDOWN_MS = 6000
// Across every plugin: state keyed on a plugin would not outlive a remount.
let quietUntil = 0

export function usePluginLinks() {
    const deviceStore = useDeviceStore()
    const dialog = useDialog()
    const toast = useToast()
    const { t } = useI18n()

    /**
     * Ask the user whether to open a link that a plugin supplied.
     *
     * `isOwnClick` is for the UI's own click handlers only, never for a plugin's message.
     */
    const requestLink = (pluginId: string, raw: unknown, isOwnClick = false): void => {
        // A request has to follow a click. The activation is the window's, so a click
        // anywhere in the UI counts, the plugin's frame included: the cooldown is what stops
        // a plugin riding the user's clicks on the prompt or the UI into a stream of prompts
        // or toasts. Browsers without the API still get the confirmation.
        if (isAsking || navigator.userActivation?.isActive === false) {
            return
        }
        if (!isOwnClick && Date.now() < quietUntil) {
            return
        }
        const url = validatePluginLinkForUi(raw, deviceStore.daemonClient.daemonURL)
        if (url == null) {
            if (!isOwnClick) quietUntil = Date.now() + PLUGIN_LINK_COOLDOWN_MS
            toast.add({
                severity: 'warn',
                summary: t('layout.plugins.linkBlocked'),
                detail: t('layout.plugins.linkBlockedDetail'),
                life: 4000,
            })
            return
        }
        isAsking = true
        dialog.open(PluginLinkDialog, {
            props: { header: t('layout.plugins.openLinkHeader') },
            data: { pluginId, url },
            onClose: () => {
                isAsking = false
                quietUntil = Date.now() + PLUGIN_LINK_COOLDOWN_MS
            },
        })
    }

    return { requestLink }
}
