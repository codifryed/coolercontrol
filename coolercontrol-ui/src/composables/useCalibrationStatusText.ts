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
import type { Calibration, CalibrationWarning } from '@/models/Calibration'

/**
 * Shared formatters for calibration result text. Used by the popover
 * panel that lists the result and by surfaces that show only a short
 * tooltip (e.g. the trigger button on the controls overview), so all
 * sites agree on whether mapping is actually active.
 *
 * The `completed` text branches three ways: warnings win (because they
 * imply the mapping is degraded or off), then `Stepped` curves (which
 * are explicit passthrough), then plain smooth. The `NoTachometer`
 * case from the daemon arrives as a Stepped record with a warning, so
 * it correctly falls into the "with warnings" branch.
 */
export function useCalibrationStatusText() {
    const { t } = useI18n()

    function warningText(warning: CalibrationWarning): string {
        switch (warning.kind) {
            case 'no_tachometer':
                return t('components.channelExtensionSettings.calibration.warningNoTachometer')
            case 'not_controllable':
                return t('components.channelExtensionSettings.calibration.warningNotControllable')
            case 'limited_range':
                return t('components.channelExtensionSettings.calibration.warningLimitedRange', {
                    span: warning.rpm_span,
                })
            case 'oscillating':
                return t('components.channelExtensionSettings.calibration.warningOscillating', {
                    lower: warning.lower_duty,
                    upper: warning.upper_duty,
                })
        }
    }

    function completedStatusText(calibration: Calibration): string {
        const warnings = calibration.warnings ?? []
        if (warnings.length > 0) {
            const messages = warnings.map(warningText).join('; ')
            return t(
                'components.channelExtensionSettings.calibration.statusCompletedWithWarnings',
                { messages },
            )
        }
        return calibration.curve_kind === 'Stepped'
            ? t('components.channelExtensionSettings.calibration.statusCompletedStepped')
            : t('components.channelExtensionSettings.calibration.statusCompleted')
    }

    return { warningText, completedStatusText }
}
