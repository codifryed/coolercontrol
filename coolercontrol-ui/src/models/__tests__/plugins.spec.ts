// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import { getPluginStatusTag, PluginStatus } from '../Plugins.ts'

const enabled = { disabled: false, restart_required: false }

describe('getPluginStatusTag', () => {
    // A device plugin added while running reports Stopped. Shown as is, it reads as a
    // failure where the only step left is a daemon restart.
    it('shows a pending restart instead of Stopped', () => {
        const tag = getPluginStatusTag({ ...enabled, restart_required: true }, PluginStatus.Stopped)

        expect(tag).toEqual({ label: 'Restart Required', severity: 'warn' })
    })

    // The daemon never asks a disabled plugin for a restart, but the tag must not depend on it.
    it('shows Disabled over a pending restart', () => {
        const tag = getPluginStatusTag(
            { disabled: true, restart_required: true },
            PluginStatus.Stopped,
        )

        expect(tag).toEqual({ label: 'Disabled', severity: 'warn' })
    })

    it('maps the service status when nothing outranks it', () => {
        expect(getPluginStatusTag(enabled, PluginStatus.Running)).toEqual({
            label: 'Running',
            severity: 'success',
        })
        expect(getPluginStatusTag(enabled, PluginStatus.Stopped)).toEqual({
            label: 'Stopped',
            severity: 'danger',
        })
        expect(getPluginStatusTag(enabled, PluginStatus.Unmanaged)).toEqual({
            label: 'Unmanaged',
            severity: 'secondary',
        })
    })
})
