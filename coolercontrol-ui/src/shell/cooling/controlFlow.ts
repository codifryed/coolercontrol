// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import type { RouteLocationRaw } from 'vue-router'
import {
    type Profile,
    ProfileType,
    getProfileMixFunctionTypeDisplayName,
    getProfileTypeDisplayName,
} from '@/models/Profile.ts'
import type { CustomSensor } from '@/models/CustomSensor.ts'

export type FlowKind = 'profile' | 'function' | 'tempSource' | 'sensor'

export interface FlowNode {
    kind: FlowKind
    label: string
    detail?: string
    to?: RouteLocationRaw
    children: Array<FlowNode>
}

export interface FlowContext {
    profiles: Array<Profile>
    functionByUID: (uid: string) => { uid: string; name: string } | undefined
    sensorLabel: (deviceUID: string, channelName: string) => string
    // The custom sensor if the given temp source is one, else undefined.
    customSensor: (deviceUID: string, channelName: string) => CustomSensor | undefined
}

// Builds the full influence tree feeding a channel from its selected profile:
// Mix/Overlay members recurse (each with their own sensor + function), Graph and
// Fixed profiles carry their temp source + function, and a custom-sensor source
// expands into its own component sensors. Guards against profile cycles.
export function buildFlowTree(
    profile: Profile,
    ctx: FlowContext,
    seen: Set<string> = new Set(),
): FlowNode {
    const node: FlowNode = {
        kind: 'profile',
        label: profile.name,
        detail: profileDetail(profile),
        to:
            profile.uid !== '0'
                ? { name: 'profiles', params: { profileUID: profile.uid } }
                : undefined,
        children: [],
    }
    if (seen.has(profile.uid)) return node
    seen.add(profile.uid)
    for (const memberUID of profile.member_profile_uids) {
        const member = ctx.profiles.find((candidate) => candidate.uid === memberUID)
        if (member != null) node.children.push(buildFlowTree(member, ctx, seen))
    }
    if (profile.temp_source != null) {
        node.children.push(
            sensorTreeNode(profile.temp_source.device_uid, profile.temp_source.temp_name, ctx),
        )
    }
    const fun = ctx.functionByUID(profile.function_uid)
    if (fun != null) {
        node.children.push({
            kind: 'function',
            label: fun.name,
            to: { name: 'functions', params: { functionUID: fun.uid } },
            children: [],
        })
    }
    return node
}

function sensorTreeNode(deviceUID: string, channelName: string, ctx: FlowContext): FlowNode {
    const custom = ctx.customSensor(deviceUID, channelName)
    if (custom != null) {
        return {
            kind: 'tempSource',
            label: ctx.sensorLabel(deviceUID, channelName),
            to: { name: 'device-custom-sensor', params: { customSensorID: channelName } },
            // Custom sensors combine several sources; one level is expanded.
            children: custom.sources.map((source) =>
                plainSensorNode(source.deviceUID, source.name, ctx),
            ),
        }
    }
    return plainSensorNode(deviceUID, channelName, ctx)
}

function plainSensorNode(deviceUID: string, channelName: string, ctx: FlowContext): FlowNode {
    const custom = ctx.customSensor(deviceUID, channelName)
    return {
        kind: custom != null ? 'tempSource' : 'sensor',
        label: ctx.sensorLabel(deviceUID, channelName),
        to:
            custom != null
                ? { name: 'device-custom-sensor', params: { customSensorID: channelName } }
                : { name: 'monitoring-sensor', params: { deviceUID, channelName } },
        children: [],
    }
}

function profileDetail(profile: Profile): string | undefined {
    if (profile.p_type === ProfileType.Mix) {
        return profile.mix_function_type != null
            ? getProfileMixFunctionTypeDisplayName(profile.mix_function_type)
            : undefined
    }
    if (profile.p_type === ProfileType.Overlay) {
        return getProfileTypeDisplayName(ProfileType.Overlay)
    }
    return undefined
}

// A connector segment per ancestor depth, drawn as CSS lines by the tree view:
// 'through' = an ancestor's vertical passing this row, 'blank' = a gap, and the
// final segment is this node's own elbow ('branch' has siblings below, 'end' is last).
export type FlowConnector = 'through' | 'blank' | 'branch' | 'end'

export interface FlowRow {
    kind: FlowKind
    label: string
    detail?: string
    to?: RouteLocationRaw
    connectors: Array<FlowConnector>
}

export function flattenFlow(root: FlowNode): Array<FlowRow> {
    const rows: Array<FlowRow> = []
    const walk = (
        node: FlowNode,
        prefix: Array<boolean>,
        isLast: boolean,
        isRoot: boolean,
    ): void => {
        const connectors: Array<FlowConnector> = []
        if (!isRoot) {
            for (const continues of prefix) connectors.push(continues ? 'through' : 'blank')
            connectors.push(isLast ? 'end' : 'branch')
        }
        rows.push({
            kind: node.kind,
            label: node.label,
            detail: node.detail,
            to: node.to,
            connectors,
        })
        const childPrefix = isRoot ? [] : [...prefix, !isLast]
        node.children.forEach((child, index) =>
            walk(child, childPrefix, index === node.children.length - 1, false),
        )
    }
    walk(root, [], true, true)
    return rows
}

// Worth an expander when a child has its own children (a composite profile or a
// custom sensor), i.e. there is structure the flat chain cannot show.
export function isFlowExpandable(root: FlowNode): boolean {
    return root.children.some((child) => child.children.length > 0)
}
