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
 * How long after a prompt closes a plugin gets at most one more prompt, and how long after a
 * blocked-link notice it gets no other. It outlasts the few seconds a click keeps the window
 * activated.
 */
export const PLUGIN_LINK_COOLDOWN_MS = 6000
/** How long plugins get no prompt after that one more, unless the UI itself gets input. */
export const PLUGIN_LINK_LOCK_MS = 15000
// Across every plugin: state keyed on a plugin would not outlive a remount.
let quietUntil = 0
let toastQuietUntil = 0
let isLocked = false
let lockedUntil = 0
let isWatchingInput = false

// Input to the UI itself lifts the lock. A plugin cannot fake it: events in its sandboxed
// frame never reach this window. Input that starts under a prompt does not count, and the
// input that closes a prompt always starts under it.
function unlockOnInput(event: Event): void {
    if (isAsking || (event as KeyboardEvent).repeat) return
    isLocked = false
}

/**
 * Whether a click in the plugin's own frame preceded its message. Undefined where the browser
 * cannot say (Firefox, Safari). Chromium has the field on every message, null unless the
 * sender asked for it, so there a message without it counts as no click.
 */
export function frameClickOf(event: MessageEvent): boolean | undefined {
    if (!('userActivation' in event)) return undefined
    const activation = event.userActivation as { isActive?: boolean } | null | undefined
    return activation?.isActive === true
}

export function usePluginLinks() {
    const deviceStore = useDeviceStore()
    const dialog = useDialog()
    const toast = useToast()
    const { t } = useI18n()

    if (!isWatchingInput) {
        isWatchingInput = true
        window.addEventListener('pointerdown', unlockOnInput, true)
        window.addEventListener('keydown', unlockOnInput, true)
    }

    /**
     * Ask the user whether to open a link that a plugin supplied.
     *
     * `isOwnClick` is for the UI's own click handlers only, never for a plugin's message.
     * `isFrameClick` is `frameClickOf` a plugin's message.
     */
    const requestLink = (
        pluginId: string,
        raw: unknown,
        isOwnClick = false,
        isFrameClick?: boolean,
    ): void => {
        // A request has to follow a click. The activation is the window's, so a click
        // anywhere in the UI counts, the plugin's frame included. Browsers without the API
        // still get the confirmation.
        if (isAsking || navigator.userActivation?.isActive === false) {
            return
        }
        // Where the browser says so, a plugin's request has to follow a click in its own
        // frame, which stops a plugin riding the user's clicks on the prompt or the UI.
        if (!isOwnClick && isFrameClick === false) {
            return
        }
        const now = Date.now()
        const url = validatePluginLinkForUi(raw, deviceStore.daemonClient.daemonURL)
        if (url == null) {
            if (!isOwnClick) {
                if (now < toastQuietUntil) return
                toastQuietUntil = now + PLUGIN_LINK_COOLDOWN_MS
            }
            toast.add({
                severity: 'warn',
                summary: t('layout.plugins.linkBlocked'),
                detail: t('layout.plugins.linkBlockedDetail'),
                life: 4000,
            })
            return
        }
        // Where the browser does not say, the riding is bounded instead: one more prompt
        // in the cooldown, so a second try at a link works, then none while locked.
        if (!isOwnClick && isFrameClick == null) {
            if (isLocked && now < lockedUntil) return
            isLocked = now < quietUntil
        }
        isAsking = true
        dialog.open(PluginLinkDialog, {
            props: { header: t('layout.plugins.openLinkHeader') },
            data: { pluginId, url },
            onClose: () => {
                isAsking = false
                quietUntil = Date.now() + PLUGIN_LINK_COOLDOWN_MS
                lockedUntil = Date.now() + PLUGIN_LINK_LOCK_MS
            },
        })
    }

    return { requestLink }
}
