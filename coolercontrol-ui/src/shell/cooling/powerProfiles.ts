// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Enumerable at runtime on purpose: each name maps to a
// `layout.shell.coolingPage.powerProfiles.profileNames.*` translation key that ModesPage only
// builds as a template literal, invisible to an unused-key sweep. The i18n spec walks this array.
//
// These are the three profiles power-profiles-daemon defines. A system offering anything else
// still works; its profile is shown under its raw name rather than a translated one.
export const POWER_PROFILE_NAMES = ['power-saver', 'balanced', 'performance'] as const

export type PowerProfileName = (typeof POWER_PROFILE_NAMES)[number]

/**
 * The Mode mapped to the active profile when it is not the active Mode, otherwise undefined.
 * The daemon activates that Mode again when it starts, replacing whatever was applied by hand
 * since, so the page says so before it happens.
 */
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

/** Whether `profile` is one of the names that has a translated label. */
export function hasTranslatedLabel(profile: string): boolean {
    return (POWER_PROFILE_NAMES as readonly string[]).includes(profile)
}
