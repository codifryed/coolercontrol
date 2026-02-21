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

import { computed, type Ref, ref } from 'vue'
import { MarkerType, type Node, type Edge } from '@vue-flow/core'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { DeviceType, type UID } from '@/models/Device'
import { ProfileType } from '@/models/Profile'
import type { CustomSensor } from '@/models/CustomSensor'

export interface FanOption {
    key: string
    label: string
}

export interface FanNodeData {
    deviceUID: UID
    channelName: string
    channelLabel: string
    channelColor: string
    deviceLabel: string
    duty?: string
    rpm?: string
    isManual: boolean
    manualDuty?: number
    profileUID?: string
}

export interface ProfileNodeData {
    profileUID: UID
    profileName: string
    profileType: ProfileType
    speedFixed?: number
    mixFunctionType?: string
    functionName?: string
    functionType?: string
    functionUID?: string
    isDefault: boolean
}

export interface TempSourceNodeData {
    deviceUID: UID
    tempName: string
    tempLabel: string
    tempColor: string
    deviceLabel: string
}

export interface CustomSensorNodeData {
    sensorId: string
    sensorName: string
    csType: string
    deviceUID: UID
}

const NODE_WIDTH = 260
const NODE_HEIGHT = 120
export const COLUMN_GAP = 350

// Fixed column assignments by node type/subtype.
// Flow moves left-to-right: Fan → Overlay → Mix → Graph → CustomSensor → TempSource
const COL_FAN = 0
const COL_OVERLAY = 1
const COL_MIX = 2
const COL_GRAPH = 3 // Also Fixed and Default profiles
const COL_CUSTOM_SENSOR = 4
const COL_TEMP_SOURCE = 5

function profileColumn(pType: ProfileType): number {
    switch (pType) {
        case ProfileType.Overlay:
            return COL_OVERLAY
        case ProfileType.Mix:
            return COL_MIX
        default:
            return COL_GRAPH
    }
}

export function useControlFlowGraph(selectedFanKey: Ref<string | undefined>) {
    const deviceStore = useDeviceStore()
    const settingsStore = useSettingsStore()
    const customSensors: Ref<Map<string, CustomSensor>> = ref(new Map())
    let customSensorsLoaded = false

    async function loadCustomSensors(): Promise<void> {
        if (customSensorsLoaded) return
        const sensors = await settingsStore.getCustomSensors()
        const map = new Map<string, CustomSensor>()
        for (const sensor of sensors) {
            map.set(sensor.id, sensor)
        }
        customSensors.value = map
        customSensorsLoaded = true
    }

    const availableFans = computed<FanOption[]>(() => {
        const fans: FanOption[] = []
        for (const device of deviceStore.allDevices()) {
            if (device.type === DeviceType.CUSTOM_SENSORS || device.type === DeviceType.CPU)
                continue
            if (device.info == null) continue
            const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)
            for (const [channelName, channelInfo] of device.info.channels.entries()) {
                if (!channelInfo.speed_options?.fixed_enabled) continue
                const channelLabel =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.name ?? channelName
                const deviceLabel = deviceSettings?.name ?? device.name
                fans.push({
                    key: `${device.uid}/${channelName}`,
                    label: `${deviceLabel} - ${channelLabel}`,
                })
            }
        }
        return fans
    })

    const graphData = computed(() => {
        const nodeMap = new Map<string, Node>()
        const edgeMap = new Map<string, Edge>()
        const nodeColumns = new Map<string, number>()
        const nodeConnections = new Map<string, Set<string>>()

        function addNode(node: Node, column: number): void {
            if (!nodeMap.has(node.id)) {
                nodeMap.set(node.id, node)
                nodeColumns.set(node.id, column)
            }
        }

        function addEdge(sourceId: string, targetId: string, isDashed: boolean = false): void {
            const edgeId = `edge::${sourceId}->${targetId}`
            if (edgeMap.has(edgeId)) return
            edgeMap.set(edgeId, {
                id: edgeId,
                source: sourceId,
                target: targetId,
                type: 'default',
                animated: true,
                markerEnd: {
                    type: MarkerType.Arrow,
                    color: 'rgb(var(--colors-accent))',
                },
                style: {
                    stroke: 'rgb(var(--colors-accent))',
                    strokeWidth: 2,
                    ...(isDashed ? { strokeDasharray: '6 4' } : {}),
                },
            })
            if (!nodeConnections.has(sourceId)) nodeConnections.set(sourceId, new Set())
            nodeConnections.get(sourceId)!.add(targetId)
            if (!nodeConnections.has(targetId)) nodeConnections.set(targetId, new Set())
            nodeConnections.get(targetId)!.add(sourceId)
        }

        function traceProfileChain(
            profileUID: UID,
            fanNodeId: string,
            visited: Set<string>,
            isDashed: boolean = false,
            depth: number = 0,
        ): string | undefined {
            if (depth > 2) return undefined
            const nodeId = `profile::${profileUID}`
            if (visited.has(nodeId)) {
                addEdge(nodeId, fanNodeId, isDashed)
                return nodeId
            }
            visited.add(nodeId)

            const profile = settingsStore.profiles.find((p) => p.uid === profileUID)
            if (!profile) return undefined

            let functionName: string | undefined
            let functionType: string | undefined
            let functionUID: string | undefined
            if (profile.function_uid !== '0') {
                const fn = settingsStore.functions.find((f) => f.uid === profile.function_uid)
                if (fn) {
                    functionName = fn.name
                    functionType = fn.f_type
                    functionUID = fn.uid
                }
            }

            const profileNode: Node = {
                id: nodeId,
                type: 'profile',
                position: { x: 0, y: 0 },
                data: {
                    profileUID: profile.uid,
                    profileName: profile.name,
                    profileType: profile.p_type,
                    speedFixed: profile.speed_fixed,
                    mixFunctionType: profile.mix_function_type,
                    functionName,
                    functionType,
                    functionUID,
                    isDefault: profile.uid === '0',
                } satisfies ProfileNodeData,
            }
            addNode(profileNode, profileColumn(profile.p_type))
            addEdge(nodeId, fanNodeId, isDashed)

            if (profile.p_type === ProfileType.Graph && profile.temp_source) {
                traceTempSource(
                    profile.temp_source.device_uid,
                    profile.temp_source.temp_name,
                    nodeId,
                    visited,
                )
            } else if (profile.p_type === ProfileType.Mix) {
                for (const memberUID of profile.member_profile_uids) {
                    traceProfileChain(memberUID, nodeId, visited, true, depth + 1)
                }
            } else if (profile.p_type === ProfileType.Overlay) {
                if (profile.member_profile_uids.length > 0) {
                    traceProfileChain(
                        profile.member_profile_uids[0],
                        nodeId,
                        visited,
                        true,
                        depth + 1,
                    )
                }
            }

            return nodeId
        }

        function traceTempSource(
            deviceUID: UID,
            tempName: string,
            parentNodeId: string,
            visited: Set<string>,
            depth: number = 0,
        ): void {
            if (depth > 2) return

            // Check if temp source is from a custom sensor device
            let isCustomSensor = false
            for (const device of deviceStore.allDevices()) {
                if (device.uid === deviceUID && device.type === DeviceType.CUSTOM_SENSORS) {
                    isCustomSensor = true
                    break
                }
            }

            if (isCustomSensor) {
                traceCustomSensor(tempName, deviceUID, parentNodeId, visited, depth)
            } else {
                const tempNodeId = `temp::${deviceUID}::${tempName}`
                if (!visited.has(tempNodeId)) {
                    visited.add(tempNodeId)
                    const deviceSettings = settingsStore.allUIDeviceSettings.get(deviceUID)
                    const tempLabel =
                        deviceSettings?.sensorsAndChannels.get(tempName)?.name ?? tempName
                    const tempColor =
                        deviceSettings?.sensorsAndChannels.get(tempName)?.color ?? '#568af2'
                    const deviceLabel = deviceSettings?.name ?? deviceUID

                    const tempNode: Node = {
                        id: tempNodeId,
                        type: 'tempSource',
                        position: { x: 0, y: 0 },
                        data: {
                            deviceUID,
                            tempName,
                            tempLabel,
                            tempColor,
                            deviceLabel,
                        } satisfies TempSourceNodeData,
                    }
                    addNode(tempNode, COL_TEMP_SOURCE)
                }
                addEdge(tempNodeId, parentNodeId)
            }
        }

        function traceCustomSensor(
            sensorId: string,
            deviceUID: UID,
            parentNodeId: string,
            visited: Set<string>,
            depth: number = 0,
        ): void {
            if (depth > 2) return
            const csNodeId = `cs::${sensorId}`
            if (!visited.has(csNodeId)) {
                visited.add(csNodeId)
                const sensor = customSensors.value.get(sensorId)
                const csNode: Node = {
                    id: csNodeId,
                    type: 'customSensor',
                    position: { x: 0, y: 0 },
                    data: {
                        sensorId,
                        sensorName: sensor?.id ?? sensorId,
                        csType: sensor?.cs_type ?? 'Unknown',
                        deviceUID,
                    } satisfies CustomSensorNodeData,
                }
                addNode(csNode, COL_CUSTOM_SENSOR)

                if (sensor) {
                    for (const source of sensor.sources) {
                        traceTempSource(
                            source.temp_source.device_uid,
                            source.temp_source.temp_name,
                            csNodeId,
                            visited,
                            depth + 1,
                        )
                    }
                }
            }
            addEdge(csNodeId, parentNodeId)
        }

        // Build graph for each controllable fan channel
        for (const device of deviceStore.allDevices()) {
            if (device.type === DeviceType.CUSTOM_SENSORS || device.type === DeviceType.CPU)
                continue
            if (device.info == null) continue

            const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)
            const daemonSettings = settingsStore.allDaemonDeviceSettings.get(device.uid)

            for (const [channelName, channelInfo] of device.info.channels.entries()) {
                if (!channelInfo.speed_options?.fixed_enabled) continue

                const fanNodeId = `fan::${device.uid}::${channelName}`
                const channelLabel =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.name ?? channelName
                const channelColor =
                    deviceSettings?.sensorsAndChannels.get(channelName)?.color ?? '#568af2'
                const deviceLabel = deviceSettings?.name ?? device.name

                const channelSetting = daemonSettings?.settings.get(channelName)
                const isManual = channelSetting?.speed_fixed != null
                const profileUID = channelSetting?.profile_uid

                const fanNode: Node = {
                    id: fanNodeId,
                    type: 'fanChannel',
                    position: { x: 0, y: 0 },
                    data: {
                        deviceUID: device.uid,
                        channelName,
                        channelLabel,
                        channelColor,
                        deviceLabel,
                        isManual,
                        manualDuty: channelSetting?.speed_fixed,
                        profileUID,
                    } satisfies FanNodeData,
                }
                addNode(fanNode, COL_FAN)

                if (!isManual) {
                    const visited = new Set<string>()
                    visited.add(fanNodeId)
                    if (profileUID != null) {
                        traceProfileChain(profileUID, fanNodeId, visited)
                    } else {
                        // Default profile
                        traceProfileChain('0', fanNodeId, visited)
                    }
                }
            }
        }

        // Filter to selected fan subgraph if needed
        if (selectedFanKey.value) {
            const [deviceUID, channelName] = selectedFanKey.value.split('/')
            const fanNodeId = `fan::${deviceUID}::${channelName}`
            if (nodeMap.has(fanNodeId)) {
                const reachable = new Set<string>()
                const queue = [fanNodeId]
                reachable.add(fanNodeId)
                while (queue.length > 0) {
                    const current = queue.shift()!
                    for (const edge of edgeMap.values()) {
                        // Only traverse upstream: edges point source→target,
                        // so follow from target back to source
                        if (edge.target === current && !reachable.has(edge.source)) {
                            reachable.add(edge.source)
                            queue.push(edge.source)
                        }
                    }
                }
                for (const nodeId of [...nodeMap.keys()]) {
                    if (!reachable.has(nodeId)) nodeMap.delete(nodeId)
                }
                for (const edgeId of [...edgeMap.keys()]) {
                    const edge = edgeMap.get(edgeId)!
                    if (!reachable.has(edge.source) || !reachable.has(edge.target)) {
                        edgeMap.delete(edgeId)
                    }
                }
            }
        }

        // Compute layout positions
        layoutNodes(nodeMap, edgeMap, nodeColumns)

        return {
            nodes: Array.from(nodeMap.values()),
            edges: Array.from(edgeMap.values()),
        }
    })

    function layoutNodes(
        nodeMap: Map<string, Node>,
        edgeMap: Map<string, Edge>,
        nodeColumns: Map<string, number>,
    ): void {
        // Group nodes by column
        const columns = new Map<number, string[]>()
        for (const [nodeId, col] of nodeColumns.entries()) {
            if (!nodeMap.has(nodeId)) continue
            if (!columns.has(col)) columns.set(col, [])
            columns.get(col)!.push(nodeId)
        }
        if (columns.size === 0) return

        // Collapse empty columns: map only populated column indices to sequential X
        const populatedCols = [...columns.keys()].sort((a, b) => a - b)
        const colToX = new Map<number, number>()
        for (let i = 0; i < populatedCols.length; i++) {
            colToX.set(populatedCols[i], i * COLUMN_GAP)
        }

        // Build downstream mapping: for each node, which fan node(s) does it serve?
        const downstreamFans = new Map<string, Set<string>>()
        const fanNodeIds = columns.get(COL_FAN) ?? []
        for (const fanId of fanNodeIds) {
            const queue = [fanId]
            const visited = new Set<string>([fanId])
            while (queue.length > 0) {
                const current = queue.shift()!
                if (!downstreamFans.has(current)) downstreamFans.set(current, new Set())
                downstreamFans.get(current)!.add(fanId)
                for (const edge of edgeMap.values()) {
                    if (edge.target === current && !visited.has(edge.source)) {
                        visited.add(edge.source)
                        queue.push(edge.source)
                    }
                }
            }
        }

        // Sort fans for stable ordering
        fanNodeIds.sort()

        // Calculate band height per fan: max exclusive nodes in any non-fan column.
        // Each fan gets a vertical "band" so its exclusive upstream nodes stay in-lane.
        const nodeSpacing = NODE_HEIGHT + 40
        const bandGap = 20
        const fanBandRows = new Map<string, number>()
        for (const fanId of fanNodeIds) {
            let maxRows = 1
            for (const [col, colNodes] of columns.entries()) {
                if (col === COL_FAN) continue
                const exclusiveCount = colNodes.filter((nid) => {
                    const fans = downstreamFans.get(nid)
                    return fans != null && fans.size === 1 && fans.has(fanId)
                }).length
                maxRows = Math.max(maxRows, exclusiveCount)
            }
            fanBandRows.set(fanId, maxRows)
        }

        // Position fan nodes with band-based vertical spacing
        const nodePositions = new Map<string, { x: number; y: number }>()
        const fanBandStartY = new Map<string, number>()
        let currentY = 0
        const fanX = colToX.get(COL_FAN) ?? 0
        for (const fanId of fanNodeIds) {
            const rows = fanBandRows.get(fanId) ?? 1
            const bandHeight = (rows - 1) * nodeSpacing
            const centerY = currentY + bandHeight / 2
            fanBandStartY.set(fanId, currentY)
            nodePositions.set(fanId, { x: fanX, y: centerY })
            currentY += bandHeight + nodeSpacing + bandGap
        }

        // Position non-fan nodes per column
        for (const col of populatedCols) {
            if (col === COL_FAN) continue
            const colNodes = columns.get(col) ?? []
            if (colNodes.length === 0) continue
            const x = colToX.get(col)!

            // Separate exclusive (single-fan) from shared (multi-fan) nodes
            const fanGrouped = new Map<string, string[]>()
            const sharedNodes: string[] = []
            for (const nodeId of colNodes) {
                const fans = downstreamFans.get(nodeId)
                if (fans != null && fans.size === 1) {
                    const fanId = [...fans][0]
                    if (!fanGrouped.has(fanId)) fanGrouped.set(fanId, [])
                    fanGrouped.get(fanId)!.push(nodeId)
                } else {
                    sharedNodes.push(nodeId)
                }
            }

            // Place exclusive nodes centered within their fan's band
            for (const [fanId, nodeIds] of fanGrouped.entries()) {
                const bandStart = fanBandStartY.get(fanId) ?? 0
                const rows = fanBandRows.get(fanId) ?? 1
                const bandHeight = (rows - 1) * nodeSpacing
                const bandCenter = bandStart + bandHeight / 2
                const groupHeight = (nodeIds.length - 1) * nodeSpacing
                const startY = bandCenter - groupHeight / 2
                for (let i = 0; i < nodeIds.length; i++) {
                    nodePositions.set(nodeIds[i], { x, y: startY + i * nodeSpacing })
                }
            }

            // Place shared nodes at the average Y of their connected fans
            for (const nodeId of sharedNodes) {
                const fans = downstreamFans.get(nodeId) ?? new Set()
                const y = avgY(fans, nodePositions)
                nodePositions.set(nodeId, { x, y })
            }

            // Resolve overlaps among shared nodes only (exclusive are pre-spaced)
            if (sharedNodes.length > 1) {
                resolveOverlaps(sharedNodes, nodePositions)
            }
        }

        // Apply positions to node objects
        for (const [nodeId, pos] of nodePositions.entries()) {
            const node = nodeMap.get(nodeId)
            if (node) {
                node.position = pos
            }
        }
    }

    function avgY(nodeIds: Set<string>, positions: Map<string, { x: number; y: number }>): number {
        if (nodeIds.size === 0) return 0
        let sum = 0
        let count = 0
        for (const id of nodeIds) {
            const pos = positions.get(id)
            if (pos) {
                sum += pos.y
                count++
            }
        }
        return count > 0 ? sum / count : 0
    }

    function resolveOverlaps(
        nodeIds: string[],
        positions: Map<string, { x: number; y: number }>,
    ): void {
        nodeIds.sort((a, b) => {
            const aY = positions.get(a)?.y ?? 0
            const bY = positions.get(b)?.y ?? 0
            return aY - bY
        })
        const minGap = NODE_HEIGHT + 40
        for (let i = 1; i < nodeIds.length; i++) {
            const prev = positions.get(nodeIds[i - 1])!
            const curr = positions.get(nodeIds[i])!
            if (curr.y - prev.y < minGap) {
                curr.y = prev.y + minGap
            }
        }
    }

    const nodes = computed(() => graphData.value.nodes)
    const edges = computed(() => graphData.value.edges)

    return {
        nodes,
        edges,
        availableFans,
        loadCustomSensors,
        NODE_WIDTH,
    }
}
