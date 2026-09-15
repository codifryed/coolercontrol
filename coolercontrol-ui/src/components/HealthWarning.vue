<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiAlertCircle } from '@mdi/js'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { DeviceType } from '@/models/Device.ts'
import { HealthEntityType, SourceRef, sourceTempDisplayName } from '@/models/DeviceHealth.ts'
import { useDeviceHealth } from '@/composables/useDeviceHealth.ts'

// Inline warning listing an entity's current device-health issues (failsafe,
// missing or stale temp sources), so its edit page shows what is wrong.
interface Props {
    kind: 'channel' | 'custom-sensor' | 'profile' | 'lcd'
    /** Device UID for channel and lcd kinds. */
    deviceUid?: string
    /** Channel name for channel and lcd kinds. */
    channelName?: string
    /** Custom Sensor id or Profile UID. */
    entityUid?: string
}
const props = defineProps<Props>()
const { t } = useI18n({ useScope: 'global' })
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { isDeviceUnreachable, unreachableText } = useDeviceHealth()

// A custom sensor's own failsafe entry lives on the Custom Sensors device
// under its sensor id; channels and LCDs failsafe under their own device.
const failsafeSubject = computed((): { deviceUid: string; channelName: string } | undefined => {
    if (props.kind === 'custom-sensor') {
        if (props.entityUid == null) return undefined
        for (const device of deviceStore.allDevices()) {
            if (device.type === DeviceType.CUSTOM_SENSORS) {
                return { deviceUid: device.uid, channelName: props.entityUid }
            }
        }
        return undefined
    }
    if (props.deviceUid == null || props.channelName == null) return undefined
    return { deviceUid: props.deviceUid, channelName: props.channelName }
})

const matchesEntity = (ref: SourceRef): boolean => {
    switch (props.kind) {
        case 'custom-sensor':
            return (
                ref.entity_type === HealthEntityType.CustomSensor &&
                ref.entity_uid === props.entityUid
            )
        case 'profile':
            return (
                ref.entity_type === HealthEntityType.Profile && ref.entity_uid === props.entityUid
            )
        case 'lcd':
            return (
                ref.entity_type === HealthEntityType.Lcd &&
                ref.entity_uid === props.deviceUid &&
                ref.channel_name === props.channelName
            )
        default:
            return false
    }
}

const sourceLine = (labelKey: string, ref: SourceRef): string =>
    `${t(labelKey)}: ${sourceTempDisplayName(ref, settingsStore.allUIDeviceSettings)}`

const issues = computed((): Array<string> => {
    const lines: Array<string> = []
    const subject = failsafeSubject.value
    if (subject != null) {
        // The device not answering is the cause of its channels' failsafe, so it is reported in
        // their place: it is what the user can act on, and it says why a setting here has no
        // effect right now.
        const unreachable = isDeviceUnreachable(subject.deviceUid)
        if (unreachable) {
            lines.push(unreachableText())
        }
        const failsafeRef = unreachable
            ? undefined
            : settingsStore.healthFailsafe.find(
                  (ref) => ref.device_uid === subject.deviceUid && ref.name === subject.channelName,
              )
        if (failsafeRef != null) {
            lines.push(
                failsafeRef.reason
                    ? `${t('views.appInfo.failsafeActive')}: ${failsafeRef.reason}`
                    : t('views.appInfo.failsafeActive'),
            )
        }
    }
    for (const ref of settingsStore.healthMissing.filter(matchesEntity)) {
        lines.push(sourceLine('views.appInfo.missingTempSource', ref))
    }
    for (const ref of settingsStore.healthStaleSource.filter(matchesEntity)) {
        lines.push(sourceLine('views.appInfo.staleTempSource', ref))
    }
    return lines
})
</script>

<template>
    <div
        v-if="issues.length > 0"
        class="flex flex-row items-center gap-2 rounded-lg border border-warning bg-warning/10 p-3"
    >
        <svg-icon
            type="mdi"
            class="text-warning min-w-6"
            :path="mdiAlertCircle"
            :size="deviceStore.getREMSize(1.25)"
        />
        <div class="flex flex-col">
            <span v-for="issue in issues" :key="issue">{{ issue }}</span>
        </div>
    </div>
</template>
