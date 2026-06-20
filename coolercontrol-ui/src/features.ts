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

// Build-time feature flags for experimental UI features. Each flag is gated to
// specific branch builds via FEATURE_BRANCHES in vite.config and hidden
// everywhere else (main, release, and builds where the branch is undetectable).

export type FeatureName = 'coolingWizard'

// Injected by vite.config from the git branch at build time.
declare const __FEATURES__: Partial<Record<FeatureName, boolean>>

const injected: Partial<Record<FeatureName, boolean>> =
    typeof __FEATURES__ !== 'undefined' ? __FEATURES__ : {}

export const features: Readonly<Record<FeatureName, boolean>> = Object.freeze({
    coolingWizard: injected.coolingWizard ?? false,
})
