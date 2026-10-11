// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { useI18n } from 'vue-i18n'
import { v4 as uuidV4 } from 'uuid'
import { useRouter } from 'vue-router'
import { ChannelMetric } from '@/models/ChannelSource.ts'

// Opens the alert editor prefilled to fire when a fan drops to 0 rpm. The editor picks the
// upper threshold, which depends on the fan's current reading.
export function useFailAlert() {
    const { t } = useI18n()
    const router = useRouter()
    const createFailAlert = (deviceUID: string, channelName: string, label: string): void => {
        router.push({
            name: 'monitoring-alert-new',
            query: {
                device: deviceUID,
                channel: channelName,
                metric: ChannelMetric.RPM,
                min: '1',
                name: `${label} ${t('layout.shell.monitoringPanel.failAlertSuffix')}`,
                // The router-view is keyed on path plus this, and the path does not change
                // between two create-alert clicks. Without it the editor is not remounted
                // and keeps the first sensor's prefill while the URL names the second.
                key: uuidV4(),
            },
        })
    }
    return { createFailAlert }
}
