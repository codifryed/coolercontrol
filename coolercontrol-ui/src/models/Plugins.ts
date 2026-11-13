// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import i18n from '@/i18n'

export default class PluginsDto {
    plugins: Array<PluginDto> = []
}

export class PluginDto {
    id: string = ''
    service_type: ServiceType = ServiceType.Integration
    description?: string
    version?: string
    url?: string
    address: String = ''
    privileged: boolean = false
    path: String = ''
    disabled: boolean = false
    // Only a daemon restart applies this plugin's changes: found, enabled or disabled since
    // startup, or a manifest edit.
    restart_required: boolean = false
}

export enum ServiceType {
    Device = 'Device',
    Integration = 'Integration',
}

export enum PluginStatus {
    Running = 'Running',
    Stopped = 'Stopped',
    Unmanaged = 'Unmanaged',
}

export function getPluginStatusDisplayName(status: PluginStatus): string {
    const { t } = i18n.global
    switch (status) {
        case PluginStatus.Running:
            return t('models.pluginStatus.running')
        case PluginStatus.Stopped:
            return t('models.pluginStatus.stopped')
        case PluginStatus.Unmanaged:
            return t('models.pluginStatus.unmanaged')
        default:
            return String(status)
    }
}

export type PluginTagSeverity = 'success' | 'danger' | 'secondary' | 'warn'

// The plugin's state tag. A pending restart outranks Disabled, since a device plugin
// disabled while running keeps its devices until then. Both outrank the service status:
// a plugin awaiting a restart can report Stopped, which would read as a failure.
export function getPluginStatusTag(
    plugin: Pick<PluginDto, 'disabled' | 'restart_required'>,
    status: PluginStatus,
): { label: string; severity: PluginTagSeverity } {
    const { t } = i18n.global
    if (plugin.restart_required) {
        return { label: t('models.pluginStatus.restartRequired'), severity: 'warn' }
    }
    if (plugin.disabled) {
        return { label: t('models.pluginStatus.disabled'), severity: 'warn' }
    }
    switch (status) {
        case PluginStatus.Running:
            return { label: getPluginStatusDisplayName(status), severity: 'success' }
        case PluginStatus.Stopped:
            return { label: getPluginStatusDisplayName(status), severity: 'danger' }
        default:
            return { label: getPluginStatusDisplayName(status), severity: 'secondary' }
    }
}

export class PluginStatusDto {
    status: PluginStatus = PluginStatus.Unmanaged
    reason?: string
}

export class HasUiDto {
    has_ui: boolean = false
}
