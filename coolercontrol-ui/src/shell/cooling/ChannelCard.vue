<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiAutoFix, mdiFanAlert } from '@mdi/js'
import { storeToRefs } from 'pinia'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceHealth } from '@/composables/useDeviceHealth.ts'
import { useFailAlert } from '@/composables/useFailAlert.ts'
import UiTooltip from '@/shell/ui/UiTooltip.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import CalibrationBadge from '@/shell/cooling/CalibrationBadge.vue'
import FirmwareCurveBadge from '@/shell/cooling/FirmwareCurveBadge.vue'
import UncontrollableBadge from '@/shell/cooling/UncontrollableBadge.vue'
import ChannelMiniGraph from '@/shell/cooling/ChannelMiniGraph.vue'
import ChannelSetupMenu from '@/shell/cooling/ChannelSetupMenu.vue'
import { HealthEntityType } from '@/models/DeviceHealth.ts'
import type { CoolingChannel } from '@/shell/cooling/channels.ts'

const props = defineProps<{ channel: CoolingChannel }>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)

const uiSetting = computed(() =>
    settingsStore.allUIDeviceSettings
        .get(props.channel.deviceUID)
        ?.sensorsAndChannels.get(props.channel.channelName),
)
const channelLabel = computed(() => uiSetting.value?.name ?? props.channel.channelName)

const { createFailAlert } = useFailAlert()
const deviceLabel = computed(
    () => settingsStore.allUIDeviceSettings.get(props.channel.deviceUID)?.name ?? '',
)
const color = computed(() => uiSetting.value?.color ?? '')

const liveDuty = computed(
    () =>
        currentDeviceStatus.value.get(props.channel.deviceUID)?.get(props.channel.channelName)
            ?.duty,
)
const liveRpm = computed(
    () =>
        currentDeviceStatus.value.get(props.channel.deviceUID)?.get(props.channel.channelName)?.rpm,
)

const daemonSetting = computed(() =>
    settingsStore.allDaemonDeviceSettings
        .get(props.channel.deviceUID)
        ?.settings.get(props.channel.channelName),
)

const assignedSummary = computed<string>(() => {
    const setting = daemonSetting.value
    if (setting?.speed_fixed != null) {
        return t('layout.shell.coolingPage.manualAt', { duty: setting.speed_fixed })
    }
    if (setting?.profile_uid != null && setting.profile_uid !== '0') {
        return (
            settingsStore.profiles.find((profile) => profile.uid === setting.profile_uid)?.name ??
            setting.profile_uid
        )
    }
    return t('common.unmanaged')
})

const health = useDeviceHealth()
const channelUnhealthy = computed(() =>
    health.isUnhealthy(props.channel.deviceUID, props.channel.channelName),
)

// A fan running a profile whose temp source is missing or stale is degraded
// even though the channel itself reports no failsafe, so fold that in: the
// panel only flags it on the profile's own row, where it is easy to miss.
const assignedProfileUID = computed(() => {
    const uid = daemonSetting.value?.profile_uid
    return uid != null && uid !== '0' ? uid : undefined
})
const profileMissingSource = computed(
    () =>
        assignedProfileUID.value != null &&
        settingsStore.healthMissing.some(
            (ref) =>
                ref.entity_type === HealthEntityType.Profile &&
                ref.entity_uid === assignedProfileUID.value,
        ),
)
const profileStaleSource = computed(
    () =>
        assignedProfileUID.value != null &&
        settingsStore.healthStaleSource.some(
            (ref) =>
                ref.entity_type === HealthEntityType.Profile &&
                ref.entity_uid === assignedProfileUID.value,
        ),
)

const isUnhealthy = computed(
    () => channelUnhealthy.value || profileMissingSource.value || profileStaleSource.value,
)

const healthTooltip = computed((): string => {
    const lines: Array<string> = []
    if (channelUnhealthy.value) {
        lines.push(health.healthTooltip(props.channel.deviceUID, props.channel.channelName))
    }
    if (profileMissingSource.value) lines.push(t('views.appInfo.missingTempSource'))
    if (profileStaleSource.value) lines.push(t('views.appInfo.staleTempSource'))
    return lines.join('\n')
})
</script>

<template>
    <RouterLink
        :to="{
            name: 'cooling-channel',
            params: { deviceUID: channel.deviceUID, channelName: channel.channelName },
        }"
        class="group flex flex-col gap-2 rounded-lg border border-border-one bg-bg-two p-3 outline-none transition-colors hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
        :style="{ borderLeftColor: color, borderLeftWidth: '3px' }"
    >
        <div class="flex items-center gap-2">
            <div class="min-w-0">
                <div class="flex items-center gap-1.5">
                    <span class="truncate font-medium text-text-color">{{ channelLabel }}</span>
                    <UiTooltip v-if="isUnhealthy" :text="healthTooltip">
                        <svg-icon
                            type="mdi"
                            :path="health.healthIcon(channel.deviceUID)"
                            :size="14"
                            class="shrink-0 text-error"
                        />
                    </UiTooltip>
                    <FirmwareCurveBadge
                        :device-u-i-d="channel.deviceUID"
                        :channel-name="channel.channelName"
                    />
                    <CalibrationBadge
                        :device-u-i-d="channel.deviceUID"
                        :channel-name="channel.channelName"
                    />
                    <UncontrollableBadge
                        :device-u-i-d="channel.deviceUID"
                        :channel-name="channel.channelName"
                    />
                </div>
                <div class="truncate text-sm text-text-color-secondary">{{ deviceLabel }}</div>
            </div>
            <div
                v-if="liveRpm != null || channel.controllable"
                class="ml-auto flex shrink-0 items-center gap-0.5"
            >
                <button
                    v-if="liveRpm != null"
                    type="button"
                    class="hidden rounded-lg p-1.5 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent group-hover:block"
                    v-tooltip.top="t('layout.shell.monitoringPanel.failAlert')"
                    @click.stop.prevent="
                        createFailAlert(channel.deviceUID, channel.channelName, channelLabel)
                    "
                >
                    <svg-icon type="mdi" :path="mdiFanAlert" :size="18" />
                </button>
                <ChannelSetupMenu
                    v-if="channel.controllable"
                    :device-u-i-d="channel.deviceUID"
                    :channel-name="channel.channelName"
                >
                    <template #trigger>
                        <button
                            type="button"
                            class="hidden rounded-lg p-1.5 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent group-hover:block data-[state=open]:block"
                            v-tooltip.top="t('layout.shell.coolingPage.guidedSetup')"
                            @click.stop.prevent
                        >
                            <svg-icon type="mdi" :path="mdiAutoFix" :size="18" />
                        </button>
                    </template>
                </ChannelSetupMenu>
            </div>
        </div>
        <div class="flex items-end justify-between gap-2">
            <div>
                <span class="text-2xl font-semibold font-numeric tabular-nums text-text-color">
                    {{ liveDuty != null ? `${liveDuty}%` : '--' }}
                </span>
                <span
                    v-if="liveRpm != null"
                    class="ml-2 text-sm font-numeric tabular-nums text-text-color-secondary"
                >
                    {{ liveRpm }} rpm
                </span>
            </div>
        </div>
        <ChannelMiniGraph
            :device-u-i-d="channel.deviceUID"
            :channel-name="channel.channelName"
            :color="color"
            class="text-text-color-secondary"
        />
        <div class="flex items-center gap-1.5 text-sm text-text-color-secondary">
            <!-- Tells a profile-driven fan apart from a fixed duty at a glance;
                 the summary line alone reads the same either way. -->
            <span
                v-if="assignedProfileUID != null"
                class="h-1.5 w-1.5 shrink-0 rounded-full bg-gradient-to-br from-accent to-accent-gradient-to"
            />
            <span class="truncate">{{ assignedSummary }}</span>
        </div>
    </RouterLink>
</template>
