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

import { useI18n } from 'vue-i18n'

const DEFAULT_PROFILE_MAX_LENGTH = 17

export interface LimitInputs {
    profileMaxLength: number
    amdGpuOverdrive?: boolean
    tempName: string
}

export interface LimitInfo {
    badge: string
    message: string
}

export function useProfileLimitInfo() {
    const { t } = useI18n()
    const getLimitInfo = (input: LimitInputs): LimitInfo | null => {
        if (input.profileMaxLength >= DEFAULT_PROFILE_MAX_LENGTH) return null
        const isAmd = input.amdGpuOverdrive === true && input.tempName === 'temp1'
        const messageKey = isAmd
            ? 'views.profiles.curveLimitedByAmdGpu'
            : 'views.profiles.curveLimitedByFirmware'
        return {
            badge: t('views.profiles.curvePointLimitBadge', { n: input.profileMaxLength }),
            message: t(messageKey, { n: input.profileMaxLength }),
        }
    }
    return { getLimitInfo }
}
