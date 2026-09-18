// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
            case 'implausible_curve':
                return t('components.channelExtensionSettings.calibration.warningImplausibleCurve')
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

    // Literal keys on purpose: building them from the stage would hide these
    // strings from the unused-key sweep and get them pruned.
    function stageLabel(stage: 'preflight' | 'up_sweep' | 'down_sweep' | 'finalizing'): string {
        switch (stage) {
            case 'preflight':
                return t('components.channelExtensionSettings.calibration.stagePreflight')
            case 'up_sweep':
                return t('components.channelExtensionSettings.calibration.stageUpSweep')
            case 'down_sweep':
                return t('components.channelExtensionSettings.calibration.stageDownSweep')
            case 'finalizing':
                return t('components.channelExtensionSettings.calibration.stageFinalizing')
        }
    }

    return { warningText, completedStatusText, stageLabel }
}
