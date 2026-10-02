// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { computed, onScopeDispose, ref, watch, type ComputedRef } from 'vue'

// Enumerable at runtime on purpose: each name maps to a
// `layout.shell.coolingPage.powerProfiles.profileNames.*` translation key that ModesPage only
// builds as a template literal, invisible to an unused-key sweep. The i18n spec walks this array.
//
// These are the three profiles power-profiles-daemon defines. A system offering anything else
// still works; its profile is shown under its raw name rather than a translated one.
export const POWER_PROFILE_NAMES = ['power-saver', 'balanced', 'performance'] as const

export type PowerProfileName = (typeof POWER_PROFILE_NAMES)[number]

/** The Mode mapped to the active profile when it is not the active Mode, otherwise undefined. */
export function unappliedProfileMode(
    activeProfile: string | undefined,
    profileModes: Record<string, string>,
    activeModeUID: string | undefined,
): string | undefined {
    if (activeProfile == null) return undefined
    const mappedModeUID = profileModes[activeProfile]
    if (mappedModeUID == null || mappedModeUID === '') return undefined
    return mappedModeUID === activeModeUID ? undefined : mappedModeUID
}

// The profile event lands before the Mode one, so an activation in flight looks like a mismatch.
export const UNAPPLIED_MODE_GRACE_MS = 3000

/**
 * `source`, but undefined until it has held a value for `graceMs` without a gap. A gap hides it
 * at once and the next value waits again; a value replaced by another one keeps showing.
 * Needs an effect scope, such as a component setup, which clears the pending timer when it ends.
 */
export function useSustained<T>(
    source: () => T | undefined,
    graceMs: number,
): ComputedRef<T | undefined> {
    const sustained = ref(false)
    let timer: ReturnType<typeof setTimeout> | undefined
    const stopTimer = (): void => {
        clearTimeout(timer)
        timer = undefined
    }
    // Sync, so the grace is measured from the edge itself and not from the next render.
    watch(
        () => source() != null,
        (present) => {
            stopTimer()
            sustained.value = false
            if (!present) return
            timer = setTimeout(() => {
                sustained.value = true
            }, graceMs)
        },
        { immediate: true, flush: 'sync' },
    )
    onScopeDispose(stopTimer)
    return computed(() => (sustained.value ? source() : undefined))
}

/** Whether `profile` is one of the names that has a translated label. */
export function hasTranslatedLabel(profile: string): boolean {
    return (POWER_PROFILE_NAMES as readonly string[]).includes(profile)
}
