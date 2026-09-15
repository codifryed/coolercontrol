<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiAlert, mdiLanDisconnect, mdiToggleSwitchOffOutline } from '@mdi/js'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Device, UID } from '@/models/Device.ts'
import { getDeviceTypeDisplayName } from '@/models/Device.ts'
import UiTooltip from '@/shell/ui/UiTooltip.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { deviceChannelLinks, deviceTypeGroups, hardwareDevices } from '@/shell/devices/devices.ts'
import { deviceTypeIcon } from '@/shell/deviceIcon.ts'
import HardwareHelpLine from '@/shell/hardware/HardwareHelpLine.vue'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const typeGroups = computed(() => deviceTypeGroups(hardwareDevices(deviceStore.allDevices())))

const disabledDevices = computed(() =>
    [...settingsStore.ccDeviceSettings.values()].filter((setting) => setting.disable),
)

const deviceLabel = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.name ?? deviceUID

// Devices without a custom color still show a dot in the theme text color.
const deviceColor = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.userColor || 'rgb(var(--colors-text-color))'

const isUnreachable = (deviceUID: UID): boolean =>
    settingsStore.healthUnreachable.some((ref) => ref.device_uid === deviceUID)

// An unreachable device's channels also failsafe, so the device state is reported instead: it is
// the cause, and "not responding" is what the user can act on.
const healthTooltip = (deviceUID: UID): string => {
    if (isUnreachable(deviceUID)) {
        return `${t('views.appInfo.deviceUnreachable')}: ${t('views.appInfo.deviceUnreachableDetail')}`
    }
    const ref = settingsStore.healthFailsafe.find((entry) => entry.device_uid === deviceUID)
    const base = t('views.appInfo.failsafeActive')
    return ref?.reason ? `${base}: ${ref.reason}` : base
}

const isUnhealthy = (deviceUID: UID): boolean =>
    isUnreachable(deviceUID) ||
    settingsStore.healthFailsafe.some((ref) => ref.device_uid === deviceUID)

const facts = (device: Device): string => {
    const parts: string[] = [getDeviceTypeDisplayName(device.type)]
    if (device.info?.model) parts.push(device.info.model)
    else if (device.info?.driver_info.name) parts.push(device.info.driver_info.name)
    return parts.join(' · ')
}

const counts = (device: Device): string => {
    const parts: string[] = []
    const temps = device.info?.temps.size ?? 0
    let fans = 0
    for (const channelInfo of device.info?.channels.values() ?? []) {
        if (channelInfo.speed_options != null) fans += 1
    }
    const links = deviceChannelLinks(device)
    const lighting = links.filter((link) => link.kind === 'lighting').length
    const lcd = links.filter((link) => link.kind === 'lcd').length
    if (temps > 0) parts.push(`${temps} ${t('layout.shell.devicesPage.temps')}`)
    if (fans > 0) parts.push(`${fans} ${t('layout.shell.devicesPage.fans')}`)
    if (lighting > 0) parts.push(`${lighting} ${t('layout.shell.devicesPage.lighting')}`)
    if (lcd > 0) parts.push(`${lcd} ${t('layout.shell.devicesPage.lcd')}`)
    return parts.join(' · ')
}
</script>

<template>
    <div class="flex h-full flex-col overflow-y-auto">
        <div class="flex flex-wrap items-center gap-3 px-4 pt-4">
            <h1 class="text-xl font-semibold text-text-color">{{ t('layout.shell.devices') }}</h1>
            <span class="text-base text-text-color-secondary">
                {{ t('layout.shell.devicesPage.landingHint') }}
            </span>
            <!-- A link cannot be a UiButton, so it carries the same outline
                 variant at the same md size by hand. Keep the two in step. -->
            <RouterLink
                :to="{ name: 'devices-manage-sensors' }"
                class="ml-auto inline-flex h-10 shrink-0 items-center gap-2 rounded-lg border border-border-one bg-control px-4 text-base font-medium text-text-color outline-none transition-colors hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiToggleSwitchOffOutline"
                    :size="18"
                    class="text-text-color-secondary"
                />
                {{ t('layout.shell.manageSensors.openButton') }}
            </RouterLink>
        </div>
        <section v-for="group in typeGroups" :key="group.type" class="px-4 pt-4">
            <h2
                class="flex items-center gap-1.5 pb-2 text-sm font-medium uppercase tracking-wide text-text-color-secondary"
            >
                <svg-icon
                    type="mdi"
                    :path="deviceTypeIcon(group.type)"
                    :size="16"
                    class="shrink-0"
                />
                <span class="truncate">{{ getDeviceTypeDisplayName(group.type) }}</span>
            </h2>
            <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
                <RouterLink
                    v-for="device in group.devices"
                    :key="device.uid"
                    :to="{ name: 'devices-device', params: { deviceUID: device.uid } }"
                    class="flex flex-col gap-1 rounded-lg border border-border-one bg-bg-two p-4 text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                >
                    <div class="flex items-center gap-2">
                        <svg-icon
                            type="mdi"
                            :path="deviceTypeIcon(device.type)"
                            :size="18"
                            class="shrink-0"
                            :style="{ color: deviceColor(device.uid) }"
                        />
                        <span class="truncate text-base font-medium">
                            {{ deviceLabel(device.uid) }}
                        </span>
                        <UiTooltip v-if="isUnhealthy(device.uid)" :text="healthTooltip(device.uid)">
                            <svg-icon
                                type="mdi"
                                :path="isUnreachable(device.uid) ? mdiLanDisconnect : mdiAlert"
                                :size="16"
                                class="shrink-0 text-error"
                            />
                        </UiTooltip>
                    </div>
                    <span class="truncate text-sm text-text-color-secondary">
                        {{ facts(device) }}
                    </span>
                    <span class="truncate text-sm text-text-color-secondary">
                        {{ counts(device) }}
                    </span>
                </RouterLink>
            </div>
        </section>
        <section v-if="disabledDevices.length > 0" class="px-4 pt-4">
            <h2
                class="truncate pb-2 text-sm font-medium uppercase tracking-wide text-text-color-secondary"
            >
                {{ t('layout.shell.devicesPanel.disabled') }}
            </h2>
            <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
                <RouterLink
                    v-for="setting in disabledDevices"
                    :key="`disabled-${setting.uid}`"
                    :to="{ name: 'devices-device', params: { deviceUID: setting.uid } }"
                    class="flex flex-col gap-1 rounded-lg border border-border-one bg-bg-two p-4 opacity-60 outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                >
                    <span class="truncate text-base font-medium text-text-color">
                        {{ setting.name }}
                    </span>
                    <span class="text-sm text-text-color-secondary">
                        {{ t('layout.shell.devicesPage.deviceDisabled') }}
                    </span>
                </RouterLink>
            </div>
        </section>
        <div class="px-4 pt-6">
            <HardwareHelpLine />
        </div>
        <div class="pb-6" />
    </div>
</template>
