<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiBellOutline,
    mdiBellRingOutline,
    mdiBellSleepOutline,
    mdiBookmarkCheck,
    mdiChartMultiple,
    mdiCircle,
    mdiCompassOutline,
    mdiFan,
    mdiKeyboardOutline,
    mdiOpenInNew,
    mdiSpeedometer,
} from '@mdi/js'
import { computed, inject, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Emitter, EventType } from 'mitt'
import type { RouteLocationRaw } from 'vue-router'
import { $enum } from 'ts-enum-util'
import { DeviceType } from '@/models/Device.ts'
import {
    ChannelVerdict,
    type FailsafeRef,
    HARDWARE_SUPPORT_DOCS,
    HealthEntityType,
    type SourceRef,
    failsafeKey,
    unreachableKey,
    sourceKey,
    sourceTempDisplayName,
} from '@/models/DeviceHealth.ts'
import { DaemonStatus, useDaemonState } from '@/stores/DaemonState.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { alertIsSilenced, getAlertStateClass, getAlertStateDisplayName } from '@/models/Alert.ts'
import { useToolWizards } from '@/composables/useToolWizards.ts'
import { channelRoute } from '@/shell/channelRoute.ts'
import { useShortcutsDialog } from '@/composables/useShortcutsDialog.ts'
import { features } from '@/features'
import StressTestsCard from '@/components/StressTestsCard.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import { detectionOpen, hardwareReportOpen } from '@/shell/hardware/hardwareModals.ts'
import { actionableFindings } from '@/shell/hardware/findings.ts'
import { useHardwareText } from '@/shell/hardware/useHardwareText.ts'

const appVersion = import.meta.env.PACKAGE_VERSION
const deviceStore = useDeviceStore()
const daemonState = useDaemonState()
const settingsStore = useSettingsStore()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
const { t } = useI18n({ useScope: 'global' })
const { openCalibrationWizard, openGenerateWizard } = useToolWizards()
const { openShortcutsDialog } = useShortcutsDialog()
const { findingDetail } = useHardwareText()

// Read from the store rather than awaiting here: a top-level await makes this an
// async-setup component behind Suspense, so a slow /health blocks navigation to Home
// entirely. The store already holds a value from startup; refresh it in the background.
const healthCheck = computed(() => daemonState.healthCheck)
onMounted(() => {
    daemonState.refreshHealth()
})

const badgeColor = computed((): string => {
    switch (daemonState.status) {
        case DaemonStatus.OK:
            return 'text-success'
        case DaemonStatus.WARN:
            return 'text-warning'
        case DaemonStatus.ERROR:
            return 'text-error'
        default:
            return 'text-error'
    }
})
// Device health has no daemon-graded severity; it is presence-of-issues.
// Mirror the card's warning-colored rows: green when clear, warning otherwise.
const healthDotColor = computed((): string =>
    healthRows.value.length === 0 ? 'text-success' : 'text-warning',
)
const getDaemonStatusTranslationKey = (daemonStatus: DaemonStatus) =>
    $enum.visitValue(daemonStatus).with<string>({
        [DaemonStatus.OK]: () => 'ok',
        [DaemonStatus.WARN]: () => 'hasWarnings',
        [DaemonStatus.ERROR]: () => 'hasErrors',
    })

const statusRows = computed((): Array<[string, string]> => [
    [t('views.appInfo.host'), healthCheck.value.system.name],
    [t('views.appInfo.uptime'), healthCheck.value.details.uptime],
    [t('views.appInfo.version'), healthCheck.value.details.version],
    [t('views.appInfo.processId'), String(healthCheck.value.details.pid)],
    [t('views.appInfo.memoryUsage'), `${healthCheck.value.details.memory_mb} MB`],
    [
        t('views.appInfo.liquidctl'),
        healthCheck.value.details.liquidctl_connected
            ? t('views.appInfo.connected')
            : t('views.appInfo.disconnected'),
    ],
])

// Device health rows, ported from AppInfoView with new-shell route targets.
interface HealthRow {
    key: string
    label: string
    detail: string
    to: RouteLocationRaw
}

const failsafeRoute = (ref: FailsafeRef): RouteLocationRaw =>
    channelRoute(deviceStore.allDevices(), ref.device_uid, ref.name)

const sourceRoute = (ref: SourceRef): RouteLocationRaw => {
    switch (ref.entity_type) {
        case HealthEntityType.CustomSensor:
            return { name: 'device-custom-sensor', params: { customSensorID: ref.entity_uid } }
        case HealthEntityType.Profile:
            return { name: 'profiles', params: { profileUID: ref.entity_uid } }
        case HealthEntityType.Lcd:
            return {
                name: 'device-lcd',
                params: { deviceUID: ref.entity_uid, channelName: ref.channel_name },
            }
    }
}

const entityTypeLabel = (type: HealthEntityType): string => {
    switch (type) {
        case HealthEntityType.CustomSensor:
            return t('layout.add.customSensor')
        case HealthEntityType.Profile:
            return t('layout.add.profile')
        case HealthEntityType.Lcd:
            return t('models.channelType.lcd')
    }
}

const customSensorsDeviceUID = computed((): string | undefined => {
    for (const device of deviceStore.allDevices()) {
        if (device.type === DeviceType.CUSTOM_SENSORS) return device.uid
    }
    return undefined
})

const customSensorLabel = (sensorId: string): string => {
    if (customSensorsDeviceUID.value == null) return sensorId
    return (
        settingsStore.allUIDeviceSettings
            .get(customSensorsDeviceUID.value)
            ?.sensorsAndChannels.get(sensorId)?.name ?? sensorId
    )
}

const sourceEntityLabel = (ref: SourceRef): string => {
    switch (ref.entity_type) {
        case HealthEntityType.CustomSensor:
            return customSensorLabel(ref.entity_uid)
        case HealthEntityType.Lcd: {
            const deviceSettings = settingsStore.allUIDeviceSettings.get(ref.entity_uid)
            const channelName =
                deviceSettings?.sensorsAndChannels.get(ref.channel_name ?? '')?.name ??
                ref.channel_name
            return `${deviceSettings?.name ?? ref.entity_name} | ${channelName}`
        }
        default:
            return ref.entity_name
    }
}

const sourceTempLabel = (ref: SourceRef): string =>
    sourceTempDisplayName(ref, settingsStore.allUIDeviceSettings)

const failsafeDetail = (ref: FailsafeRef): string =>
    ref.reason
        ? `${t('views.appInfo.failsafeActive')}: ${ref.reason}`
        : t('views.appInfo.failsafeActive')

const healthRows = computed((): Array<HealthRow> => {
    const rows: Array<HealthRow> = []
    const failsafedCustomSensors = new Set<string>()
    // A device that stopped answering fails every one of its channels over, so one row for the
    // device says what all of them would, and links to the device rather than to a channel that
    // cannot be driven either way.
    const unreachableDevices = new Set<string>()
    for (const ref of settingsStore.healthUnreachable) {
        unreachableDevices.add(ref.device_uid)
        const deviceSettings = settingsStore.allUIDeviceSettings.get(ref.device_uid)
        rows.push({
            key: `unreachable/${unreachableKey(ref)}`,
            label: deviceSettings?.name ?? ref.device_name,
            detail: `${t('views.appInfo.deviceUnreachable')}: ${t('views.appInfo.deviceUnreachableDetail')}`,
            to: { name: 'devices-device', params: { deviceUID: ref.device_uid } },
        })
    }
    for (const ref of settingsStore.healthFailsafe) {
        if (unreachableDevices.has(ref.device_uid)) continue
        const deviceSettings = settingsStore.allUIDeviceSettings.get(ref.device_uid)
        const channelName = deviceSettings?.sensorsAndChannels.get(ref.name)?.name ?? ref.name
        if (customSensorsDeviceUID.value === ref.device_uid) {
            failsafedCustomSensors.add(ref.name)
        }
        rows.push({
            key: `failsafe/${failsafeKey(ref)}`,
            label: `${deviceSettings?.name ?? ref.device_uid} | ${channelName}`,
            detail: failsafeDetail(ref),
            to: failsafeRoute(ref),
        })
    }
    for (const ref of settingsStore.healthMissing) {
        // A custom sensor with a missing source is also failsafed with that source in
        // its reason, and both rows link to the same editor, so skip the duplicate.
        if (
            ref.entity_type === HealthEntityType.CustomSensor &&
            failsafedCustomSensors.has(ref.entity_uid)
        ) {
            continue
        }
        rows.push({
            key: `missing/${sourceKey(ref)}`,
            label: `${entityTypeLabel(ref.entity_type)}: ${sourceEntityLabel(ref)}`,
            detail: `${t('views.appInfo.missingTempSource')}: ${sourceTempLabel(ref)}`,
            to: sourceRoute(ref),
        })
    }
    for (const ref of settingsStore.healthStaleSource) {
        rows.push({
            key: `stale-source/${sourceKey(ref)}`,
            label: `${entityTypeLabel(ref.entity_type)}: ${sourceEntityLabel(ref)}`,
            detail: `${t('views.appInfo.staleTempSource')}: ${sourceTempLabel(ref)}`,
            to: sourceRoute(ref),
        })
    }
    return rows
})

// Permanent hardware facts, kept in their own card. These are not faults and
// must not sit in the health list: a capability shown among failures implies
// the user should wait for it to clear, which it never will.
interface SupportRow {
    key: string
    label: string
    detail: string
    to?: RouteLocationRaw
    href?: string
}

const capabilityDetail = (verdict: ChannelVerdict): string => {
    switch (verdict) {
        case ChannelVerdict.FirmwareOverride:
            return t('layout.shell.coolingPage.verdictFirmwareOverride')
        case ChannelVerdict.FamilyMayNeedOutOfTree:
            return t('layout.shell.coolingPage.verdictFamilyMayNeedOutOfTree')
        case ChannelVerdict.NotSupportedByDriver:
            return t('layout.shell.coolingPage.verdictNotSupportedByDriver')
        case ChannelVerdict.NoPwm:
            return t('layout.shell.coolingPage.verdictNoPwm')
        case ChannelVerdict.PwmReadOnly:
            return t('layout.shell.coolingPage.verdictPwmReadOnly')
        case ChannelVerdict.IgnoresDuty:
            return t('layout.shell.coolingPage.verdictIgnoresDuty')
        case ChannelVerdict.Unverifiable:
            return t('layout.shell.coolingPage.verdictUnverifiable')
        default:
            return t('layout.shell.coolingPage.notControllable')
    }
}

// The report is useful even when nothing is wrong, so its button is not gated
// on there being findings.
// Hoisted to shell level so the search palette can raise them from any page;
// the buttons below set the same refs.

const supportRows = computed((): Array<SupportRow> => {
    const rows: Array<SupportRow> = []
    for (const finding of actionableFindings(settingsStore.healthSystemFindings)) {
        // Machine-scope, so there is no channel to route to. The docs page is
        // the only useful destination.
        rows.push({
            key: `finding/${finding.kind}/${finding.chip_name ?? finding.driver ?? ''}`,
            label: finding.chip_name ?? finding.driver ?? t('views.appInfo.hardwareSupport'),
            detail: findingDetail(finding),
            href: HARDWARE_SUPPORT_DOCS,
        })
    }
    for (const ref of settingsStore.healthChannelCapabilities) {
        const deviceSettings = settingsStore.allUIDeviceSettings.get(ref.device_uid)
        const channelName =
            deviceSettings?.sensorsAndChannels.get(ref.channel_name)?.name ?? ref.channel_name
        rows.push({
            key: `capability/${ref.device_uid}/${ref.channel_name}`,
            label: `${deviceSettings?.name ?? ref.device_uid} | ${channelName}`,
            detail: capabilityDetail(ref.verdict),
            to: channelRoute(deviceStore.allDevices(), ref.device_uid, ref.channel_name),
        })
    }
    return rows
})

const activeMode = computed(() =>
    settingsStore.modes.find((mode) => mode.uid === settingsStore.modeActiveCurrent),
)
// Error belongs here too: an alert whose sensors went unreadable cannot be
// trusted, and it lights every other surface. Silenced ones stay listed, muted.
const activeAlerts = computed(() =>
    settingsStore.alerts.filter(
        (alert) =>
            alert.enabled &&
            (settingsStore.alertsActive.includes(alert.uid) ||
                settingsStore.alertsError.includes(alert.uid)),
    ),
)

const startTour = (): void => {
    emitter.emit('start-tour')
}

const cardClasses = 'rounded-lg border border-border-one bg-bg-two p-4'
const cardTitleClasses = 'pb-3 text-lg font-medium text-text-color'
const shortcutClasses =
    'flex items-center gap-2 rounded-lg px-2 py-1.5 text-base text-accent outline-none ' +
    'hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent cursor-pointer ' +
    // text-left: a button centers its label, an anchor does not. Without this the
    // two kinds of shortcut disagree the moment a label takes a second line.
    '[&>svg]:shrink-0 min-w-0 text-left'
</script>

<template>
    <div class="flex h-full flex-col overflow-y-auto p-4">
        <div class="flex flex-wrap items-baseline gap-3">
            <h1 class="text-xl font-semibold text-text-color">{{ t('layout.shell.home') }}</h1>
            <span class="text-base text-text-color-secondary">{{ healthCheck.system.name }}</span>
            <span class="ml-auto flex flex-wrap items-baseline gap-3 text-sm">
                <a
                    href="https://gitlab.com/coolercontrol/coolercontrol/-/releases"
                    target="_blank"
                    class="text-accent"
                >
                    CoolerControl v{{ appVersion }}
                </a>
                <RouterLink
                    :to="{ name: 'settings', params: { tabNumber: '0' } }"
                    class="text-accent"
                >
                    {{ t('views.appInfo.changeStartupPage') }}
                </RouterLink>
                <span class="text-text-color-secondary">
                    {{ t('views.appInfo.noWarranty') }}
                </span>
            </span>
        </div>

        <div class="grid grid-cols-1 gap-4 pt-4 xl:grid-cols-2">
            <!-- Daemon status -->
            <div :class="cardClasses">
                <div class="flex flex-wrap items-center justify-between gap-2 pb-3">
                    <span class="text-lg font-medium text-text-color">
                        {{ t('views.appInfo.daemonStatus') }}
                    </span>
                    <div class="flex flex-wrap items-center gap-2">
                        <UiButton
                            size="sm"
                            variant="outline"
                            :disabled="daemonState.status === DaemonStatus.OK"
                            @click="daemonState.acknowledgeLogIssues()"
                        >
                            {{ t('views.appInfo.acknowledgeIssues') }}
                        </UiButton>
                        <RouterLink :to="{ name: 'home-logs' }">
                            <UiButton size="sm" variant="outline">
                                {{ t('layout.shell.homePage.viewLogs') }}
                            </UiButton>
                        </RouterLink>
                    </div>
                </div>
                <div class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1 text-base">
                    <span class="text-text-color-secondary">
                        {{ t('views.appInfo.status') }}
                    </span>
                    <RouterLink
                        :to="{
                            name: 'home-logs',
                            query: daemonState.status === DaemonStatus.OK ? {} : { level: 'warn' },
                        }"
                        class="flex w-fit items-center gap-2 rounded text-text-color outline-none hover:text-accent focus-visible:ring-2 focus-visible:ring-accent"
                        v-tooltip.top="t('layout.shell.homePage.viewLogs')"
                    >
                        <svg-icon type="mdi" :path="mdiCircle" :size="12" :class="badgeColor" />
                        {{
                            daemonState.connected
                                ? t(
                                      `daemon.status.${getDaemonStatusTranslationKey(daemonState.status)}`,
                                  )
                                : t('views.appInfo.disconnected')
                        }}
                    </RouterLink>
                    <template v-for="[label, value] in statusRows" :key="label">
                        <span class="text-text-color-secondary">{{ label }}</span>
                        <span class="text-text-color">{{ value }}</span>
                    </template>
                </div>
            </div>

            <!-- Device health -->
            <div :class="cardClasses">
                <span class="flex items-center gap-2" :class="cardTitleClasses">
                    {{ t('views.appInfo.deviceHealth') }}
                    <svg-icon type="mdi" :path="mdiCircle" :size="12" :class="healthDotColor" />
                </span>
                <div
                    v-if="healthRows.length === 0"
                    class="pt-3 text-base text-text-color-secondary"
                >
                    {{ t('views.appInfo.deviceHealthOk') }}
                </div>
                <div v-else class="flex flex-col gap-1 pt-3">
                    <RouterLink
                        v-for="row in healthRows"
                        :key="row.key"
                        :to="row.to"
                        class="rounded-lg px-2 py-1.5 outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                    >
                        <span class="flex items-center gap-2 text-base text-text-color">
                            <svg-icon
                                type="mdi"
                                :path="mdiCircle"
                                :size="10"
                                class="text-warning"
                            />
                            {{ row.label }}
                        </span>
                        <span class="block pl-5 text-sm text-text-color-secondary">
                            {{ row.detail }}
                        </span>
                    </RouterLink>
                </div>
            </div>

            <!-- Mode and alerts -->
            <div :class="cardClasses">
                <span :class="cardTitleClasses">{{
                    t('layout.shell.homePage.modeAndAlerts')
                }}</span>
                <div class="flex flex-col gap-1 pt-3">
                    <RouterLink
                        v-if="activeMode != null"
                        :to="{ name: 'modes', params: { modeUID: activeMode.uid } }"
                        :class="shortcutClasses"
                    >
                        <svg-icon type="mdi" :path="mdiBookmarkCheck" :size="18" />
                        <span class="text-text-color">{{ activeMode.name }}</span>
                        <span class="text-sm text-text-color-secondary">
                            {{ t('layout.shell.coolingPage.activeMode') }}
                        </span>
                    </RouterLink>
                    <RouterLink v-else :to="{ name: 'cooling-modes' }" :class="shortcutClasses">
                        <svg-icon type="mdi" :path="mdiBookmarkCheck" :size="18" />
                        {{ t('layout.shell.homePage.noActiveMode') }}
                    </RouterLink>
                    <RouterLink
                        v-for="alert in activeAlerts"
                        :key="alert.uid"
                        :to="{ name: 'monitoring-alert', params: { alertUID: alert.uid } }"
                        :class="shortcutClasses"
                    >
                        <!-- Shape encodes silenced, color keeps the live state: a
                             ringing bell for firing, a red sleep bell for firing-but-muted. -->
                        <svg-icon
                            type="mdi"
                            :path="
                                alertIsSilenced(alert) ? mdiBellSleepOutline : mdiBellRingOutline
                            "
                            :size="18"
                            :class="getAlertStateClass(alert.state)"
                        />
                        <span class="text-text-color">{{ alert.name }}</span>
                        <span class="text-sm" :class="getAlertStateClass(alert.state)">{{
                            getAlertStateDisplayName(alert.state)
                        }}</span>
                    </RouterLink>
                    <RouterLink :to="{ name: 'monitoring-alerts' }" :class="shortcutClasses">
                        <svg-icon type="mdi" :path="mdiBellOutline" :size="18" />
                        {{ t('views.alerts.alertsOverview') }}
                    </RouterLink>
                </div>
            </div>

            <!-- Shortcuts, grouped: tasks / learning / external resources -->
            <div :class="cardClasses">
                <span :class="cardTitleClasses">{{ t('views.appInfo.helpfulLinks') }}</span>
                <!-- auto-fit, not a viewport breakpoint: this card lives in a splitter
                     pane, so the column count has to follow the pane. Dropping a
                     column reads better than wrapping every label. -->
                <div class="grid gap-4 pt-3 grid-cols-[repeat(auto-fit,minmax(13rem,1fr))]">
                    <div class="flex flex-col gap-1">
                        <span class="px-2 pb-1 text-xs uppercase text-text-color-secondary">
                            {{ t('layout.shell.homePage.getStartedGroup') }}
                        </span>
                        <RouterLink :to="{ name: 'section-cooling' }" :class="shortcutClasses">
                            <svg-icon type="mdi" :path="mdiFan" :size="18" />
                            {{ t('layout.shell.homePage.setUpCooling') }}
                        </RouterLink>
                        <button
                            v-if="features.coolingWizard"
                            type="button"
                            :class="shortcutClasses"
                            @click="openGenerateWizard()"
                        >
                            <svg-icon type="mdi" :path="mdiChartMultiple" :size="18" />
                            {{ t('views.appInfo.gettingStartedAutoCreateLink') }}
                        </button>
                        <button
                            type="button"
                            :class="shortcutClasses"
                            @click="openCalibrationWizard()"
                        >
                            <svg-icon type="mdi" :path="mdiSpeedometer" :size="18" />
                            {{ t('views.appInfo.calibrateFansLink') }}
                        </button>
                    </div>
                    <div class="flex flex-col gap-1">
                        <span class="px-2 pb-1 text-xs uppercase text-text-color-secondary">
                            {{ t('layout.shell.homePage.learnGroup') }}
                        </span>
                        <button type="button" :class="shortcutClasses" @click="startTour">
                            <svg-icon type="mdi" :path="mdiCompassOutline" :size="18" />
                            {{ t('views.appInfo.uiTour') }}
                        </button>
                        <button
                            type="button"
                            :class="shortcutClasses"
                            @click="openShortcutsDialog()"
                        >
                            <svg-icon type="mdi" :path="mdiKeyboardOutline" :size="18" />
                            {{ t('views.shortcuts.shortcuts') }}
                        </button>
                        <a
                            href="https://docs.coolercontrol.org/getting-started.html"
                            target="_blank"
                            :class="shortcutClasses"
                        >
                            <svg-icon type="mdi" :path="mdiOpenInNew" :size="18" />
                            {{ t('views.appInfo.gettingStarted') }}
                        </a>
                    </div>
                    <div class="flex flex-col gap-1">
                        <span class="px-2 pb-1 text-xs uppercase text-text-color-secondary">
                            {{ t('layout.shell.homePage.resourcesGroup') }}
                        </span>
                        <a
                            href="https://docs.coolercontrol.org/hardware-support.html"
                            target="_blank"
                            :class="shortcutClasses"
                        >
                            <svg-icon type="mdi" :path="mdiOpenInNew" :size="18" />
                            {{ t('views.appInfo.hardwareSupport') }}
                        </a>
                        <a
                            href="https://gitlab.com/coolercontrol/coolercontrol/-/releases"
                            target="_blank"
                            :class="shortcutClasses"
                        >
                            <svg-icon type="mdi" :path="mdiOpenInNew" :size="18" />
                            {{ t('views.appInfo.whatsNew') }}
                        </a>
                        <a
                            href="https://discord.gg/MbcgUFAfhV"
                            target="_blank"
                            :class="shortcutClasses"
                        >
                            <svg-icon type="mdi" :path="mdiOpenInNew" :size="18" />
                            Discord
                        </a>
                    </div>
                </div>
            </div>

            <!-- Hardware support: permanent facts, deliberately its own card so a
                 capability is never read as a fault waiting to clear. The rows
                 are hidden when there is nothing to say, but the report stays
                 reachable because it is what the mods will ask for. -->
            <div :class="cardClasses">
                <div class="flex flex-wrap items-center justify-between gap-2 pb-3">
                    <span class="text-lg font-medium text-text-color">
                        {{ t('views.appInfo.hardwareSupport') }}
                    </span>
                    <div class="flex flex-wrap items-center gap-2">
                        <UiButton size="sm" variant="outline" @click="detectionOpen = true">
                            {{ t('views.appInfo.detectionButton') }}
                        </UiButton>
                        <UiButton size="sm" variant="outline" @click="hardwareReportOpen = true">
                            {{ t('views.appInfo.hardwareReportButton') }}
                        </UiButton>
                    </div>
                </div>
                <div v-if="supportRows.length === 0" class="text-base text-text-color-secondary">
                    {{ t('views.appInfo.hardwareSupportOk') }}
                </div>
                <div v-else class="flex flex-col gap-1">
                    <RouterLink
                        v-for="row in supportRows.filter((r) => r.to)"
                        :key="row.key"
                        :to="row.to!"
                        class="rounded-lg px-2 py-1.5 outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                    >
                        <span class="text-base text-text-color">{{ row.label }}</span>
                        <span class="block text-sm text-text-color-secondary">
                            {{ row.detail }}
                        </span>
                    </RouterLink>
                    <a
                        v-for="row in supportRows.filter((r) => r.href)"
                        :key="row.key"
                        :href="row.href"
                        target="_blank"
                        rel="noopener noreferrer"
                        class="rounded-lg px-2 py-1.5 outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                    >
                        <span class="text-base text-text-color">{{ row.label }}</span>
                        <span class="block text-sm text-text-color-secondary">
                            {{ row.detail }}
                        </span>
                    </a>
                </div>
            </div>

            <!-- Stress tests -->
            <StressTestsCard />
        </div>
        <div class="pb-8" />
    </div>
</template>
