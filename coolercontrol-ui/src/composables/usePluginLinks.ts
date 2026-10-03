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

export function usePluginLinks() {
    const deviceStore = useDeviceStore()
    const dialog = useDialog()
    const toast = useToast()
    const { t } = useI18n()

    /** Ask the user whether to open a link that a plugin supplied. */
    const requestLink = (pluginId: string, raw: unknown): void => {
        // A request has to follow a click, so a plugin cannot raise prompts by itself. A
        // click inside the plugin's frame counts here too. Browsers without the API still
        // get the confirmation.
        if (isAsking || navigator.userActivation?.isActive === false) {
            return
        }
        const url = validatePluginLinkForUi(raw, deviceStore.daemonClient.daemonURL)
        if (url == null) {
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
            },
        })
    }

    return { requestLink }
}
