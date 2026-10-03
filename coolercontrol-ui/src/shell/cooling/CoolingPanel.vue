<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiAlert,
    mdiChartMultiple,
    mdiDragVertical,
    mdiFan,
    mdiFanAlert,
    mdiFunction,
    mdiPinOff,
    mdiPinOutline,
} from '@mdi/js'
import PanelHeader from '@/shell/PanelHeader.vue'
import UiTooltip from '@/shell/ui/UiTooltip.vue'
import { VueDraggable } from 'vue-draggable-plus'
import { storeToRefs } from 'pinia'
import { computed, ref, watchEffect } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ChannelValues } from '@/stores/DeviceStore.ts'
import type { Color, UID } from '@/models/Device.ts'
import CCColorPicker from '@/components/CCColorPicker.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { HealthEntityType } from '@/models/DeviceHealth.ts'
import { useLibraryWizards } from '@/composables/useLibraryWizards.ts'
import {
    coolingChannels,
    pinId,
    type CoolingChannel,
    type CoolingDeviceGroup,
} from '@/shell/cooling/channels.ts'
import { reorderSubset, reorderTopLevel, setDeviceChildrenSubset } from '@/shell/panelOrder.ts'
import { sortEntitiesByTree } from '@/shell/libraryFolders.ts'
import LibraryList from '@/shell/cooling/LibraryList.vue'
import UiSeparator from '@/shell/ui/UiSeparator.vue'
import { channelSpins } from '@/shell/channelIcon.ts'
import { useFailAlert } from '@/composables/useFailAlert.ts'
import TagChips from '@/shell/TagChips.vue'
import TagPopover from '@/shell/monitoring/TagPopover.vue'
import HardwareHelpLine from '@/shell/hardware/HardwareHelpLine.vue'
import { useRouteActive } from '@/shell/routeActive.ts'
import { groupDigits } from '@/shell/digitGroups.ts'
import type { RouteLocationRaw } from 'vue-router'
import ChannelHealthIcon from '@/shell/ChannelHealthIcon.vue'

const { t } = useI18n()
const { createFailAlert: pushFailAlert } = useFailAlert()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)

// Mutable copy so rows are drag-sortable; rebuilt when devices change.
const groups = ref<CoolingDeviceGroup[]>([])
watchEffect(() => {
    groups.value = coolingChannels(deviceStore.allDevices(), settingsStore.channelUnit)
})

// All channel ids of a device in current order (fans are a subset of the
// device's shared order list, which also orders temps for Monitoring).
const allChannelIds = (deviceUID: UID): string[] => {
    const device = [...deviceStore.allDevices()].find((dev) => dev.uid === deviceUID)
    if (device?.info == null) return []
    const ids = [...device.info.temps.keys(), ...device.info.channels.keys()]
    return ids.map((name) => pinId(deviceUID, name))
}

const persistChannelOrder = (group: CoolingDeviceGroup): void => {
    settingsStore.menuOrder = setDeviceChildrenSubset(
        settingsStore.menuOrder,
        group.deviceUID,
        group.channels.map((channel) => pinId(channel.deviceUID, channel.channelName)),
        allChannelIds(group.deviceUID),
    )
    deviceStore.reSortDevicesByMenuOrder()
}

const deviceLabel = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.name ?? deviceUID

const deviceColor = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.userColor ?? ''

const colorStore = useThemeColorsStore()
const devicePickerColor = (deviceUID: UID): string =>
    deviceColor(deviceUID) || `rgb(${colorStore.themeColors.text_color})`
const setDeviceColor = (deviceUID: UID, newColor: Color): void => {
    const setting = settingsStore.allUIDeviceSettings.get(deviceUID)
    if (setting != null) setting.userColor = newColor
}

// Held open while the color popover is: the pointer leaves the header for the
// portalled content, which would otherwise hide the trigger under it.
const openColorDevice = ref<UID | undefined>(undefined)

// The header names a device, so it goes where the device does, and carries the
// same two actions its row in the Devices panel has.
const deviceTarget = (deviceUID: UID): RouteLocationRaw => ({
    name: 'devices-device',
    params: { deviceUID },
})
// One device order, shared by every panel. Only the devices this panel lists
// move within it; the rest keep their slots.
const persistDeviceOrder = (): void => {
    settingsStore.menuOrder = reorderTopLevel(
        settingsStore.menuOrder,
        groups.value.map((group) => group.deviceUID),
    )
    deviceStore.reSortDevicesByMenuOrder()
}

const channelLabel = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.name ??
    channelName

const channelColor = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.color ??
    ''

const liveFor = (deviceUID: UID, channelName: string): ChannelValues | undefined =>
    currentDeviceStatus.value.get(deviceUID)?.get(channelName)

// Fail-alert convenience, same as the Monitoring panel's fan rows.
const hasRpm = (channel: CoolingChannel): boolean =>
    liveFor(channel.deviceUID, channel.channelName)?.rpm != null
const liveSpeedText = (channel: CoolingChannel): string => {
    const rpm = groupDigits(liveFor(channel.deviceUID, channel.channelName)?.rpm ?? '')
    return `${rpm} ${settingsStore.rpmUnit(channel.deviceUID, channel.channelName)}`
}

const createFailAlert = (channel: CoolingChannel): void =>
    pushFailAlert(
        channel.deviceUID,
        channel.channelName,
        channelLabel(channel.deviceUID, channel.channelName),
    )

const isPinned = (channel: CoolingChannel): boolean =>
    settingsStore.pinnedIds.includes(pinId(channel.deviceUID, channel.channelName))

const togglePin = (channel: CoolingChannel): void => {
    const id = pinId(channel.deviceUID, channel.channelName)
    settingsStore.pinnedIds = settingsStore.pinnedIds.includes(id)
        ? settingsStore.pinnedIds.filter((pinned) => pinned !== id)
        : [...settingsStore.pinnedIds, id]
}

const pinnedChannels = ref<CoolingChannel[]>([])
watchEffect(() => {
    const channels = groups.value
        .flatMap((group) => group.channels)
        .filter((channel) => isPinned(channel))
    const order = settingsStore.pinnedIds
    channels.sort(
        (a, b) =>
            order.indexOf(pinId(a.deviceUID, a.channelName)) -
            order.indexOf(pinId(b.deviceUID, b.channelName)),
    )
    pinnedChannels.value = channels
})

const persistPinnedOrder = (): void => {
    settingsStore.pinnedIds = reorderSubset(
        settingsStore.pinnedIds,
        pinnedChannels.value.map((channel) => pinId(channel.deviceUID, channel.channelName)),
    )
}

const setChannelColor = (channel: CoolingChannel, newColor: Color): void => {
    const setting = settingsStore.allUIDeviceSettings
        .get(channel.deviceUID)
        ?.sensorsAndChannels.get(channel.channelName)
    if (setting != null) setting.userColor = newColor
}
const channelDefaultColor = (deviceUID: UID, channelName: string): Color | undefined =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)
        ?.defaultColor
// Reset clears the user override so the non-user-defined color applies again.
const resetChannelColor = (channel: CoolingChannel): void => {
    const setting = settingsStore.allUIDeviceSettings
        .get(channel.deviceUID)
        ?.sensorsAndChannels.get(channel.channelName)
    if (setting != null) setting.userColor = undefined
}

// Keeps a fan row's hover actions visible while its tag popover is open.
const openTagRow = ref<string | null>(null)
const onTagOpen = (rowKey: string, open: boolean): void => {
    openTagRow.value = open ? rowKey : null
}

// The default profile and function are not the user's to file or reorder.
const profileLinks = computed(() => settingsStore.profiles.filter((profile) => profile.uid !== '0'))
const functionLinks = computed(() => settingsStore.functions.filter((fun) => fun.uid !== '0'))
const sortProfiles = (): void =>
    sortEntitiesByTree(settingsStore.menuOrder, 'profiles', settingsStore.profiles, (p) => p.uid)
const sortFunctions = (): void =>
    sortEntitiesByTree(settingsStore.menuOrder, 'functions', settingsStore.functions, (f) => f.uid)
const { openProfileWizard, openFunctionWizard } = useLibraryWizards()

// A profile is unhealthy when the daemon reports a missing or stale temp source for it.
const profileTooltip = (profileUID: string): string =>
    settingsStore.healthMissing.some(
        (ref) => ref.entity_type === HealthEntityType.Profile && ref.entity_uid === profileUID,
    )
        ? t('views.appInfo.missingTempSource')
        : t('views.appInfo.staleTempSource')

const isProfileUnhealthy = (profileUID: string): boolean =>
    settingsStore.healthMissing.some(
        (ref) => ref.entity_type === HealthEntityType.Profile && ref.entity_uid === profileUID,
    ) ||
    settingsStore.healthStaleSource.some(
        (ref) => ref.entity_type === HealthEntityType.Profile && ref.entity_uid === profileUID,
    )

// The row wrapper needs the same target its link uses, so both read it here.
const channelTarget = (channel: { deviceUID: UID; channelName: string }): RouteLocationRaw => ({
    name: 'cooling-channel',
    params: { deviceUID: channel.deviceUID, channelName: channel.channelName },
})
const isRouteActive = useRouteActive()
</script>

<template>
    <div class="flex flex-col gap-0.5 p-2 pb-24 text-base">
        <template v-if="pinnedChannels.length > 0">
            <PanelHeader :label="t('layout.shell.coolingPanel.pinned')" />
            <VueDraggable
                :force-auto-scroll-fallback="true"
                v-model="pinnedChannels"
                handle=".drag-handle"
                :animation="150"
                class="flex flex-col gap-0.5"
                data-panel-pinned
                @end="persistPinnedOrder"
            >
                <div
                    v-for="channel in pinnedChannels"
                    :key="`pin-${channel.deviceUID}-${channel.channelName}`"
                    class="group flex items-center rounded-lg hover:bg-surface-hover has-[:focus-visible]:bg-surface-hover has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent"
                    :class="{ 'bg-surface-hover': isRouteActive(channelTarget(channel)) }"
                >
                    <RouterLink
                        :to="channelTarget(channel)"
                        class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none"
                        exact-active-class="!text-accent"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiFan"
                            :size="18"
                            class="shrink-0"
                            :class="{
                                'animate-spin-slow': channelSpins(
                                    'fan',
                                    liveFor(channel.deviceUID, channel.channelName),
                                    settingsStore.eyeCandy,
                                ),
                            }"
                            :style="{
                                color:
                                    channelColor(channel.deviceUID, channel.channelName) ||
                                    undefined,
                            }"
                        />
                        <span class="truncate">
                            {{ channelLabel(channel.deviceUID, channel.channelName) }}
                        </span>
                        <TagChips
                            :device-u-i-d="channel.deviceUID"
                            :channel-name="channel.channelName"
                        />
                        <!-- Huge shrink so the device name fully collapses before
                             the channel name (truncate, shrink-1) gives up any space. -->
                        <span class="shrink-[9999] truncate text-xs text-text-color-secondary">
                            {{ deviceLabel(channel.deviceUID) }}
                        </span>
                        <ChannelHealthIcon
                            :device-u-i-d="channel.deviceUID"
                            :channel-name="channel.channelName"
                        />
                        <span
                            class="ml-auto flex items-baseline gap-1.5 whitespace-nowrap group-hover:hidden group-has-[:focus-visible]:hidden"
                            :class="{
                                '!hidden':
                                    openTagRow === `${channel.deviceUID}-${channel.channelName}`,
                            }"
                        >
                            <span
                                v-if="liveFor(channel.deviceUID, channel.channelName)?.duty != null"
                                class="font-numeric tabular-nums text-text-color"
                            >
                                {{ liveFor(channel.deviceUID, channel.channelName)?.duty }}%
                            </span>
                            <span
                                v-if="liveFor(channel.deviceUID, channel.channelName)?.rpm != null"
                                class="text-sm font-numeric tabular-nums text-text-color-secondary"
                            >
                                {{ liveSpeedText(channel) }}
                            </span>
                        </span>
                    </RouterLink>
                    <!-- Every panel orders its actions by how many row types carry
                         them, most universal last. The cluster is right-aligned, so
                         only the last slot sits at a fixed x: drag is in every row,
                         so it ends every cluster, and pin precedes it. Slots are
                         24px on a 26px pitch, which is why the 20px color trigger
                         is centred in a w-6 span rather than sized up. -->
                    <div
                        class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex group-has-[[data-state=open]]:flex"
                        :class="{
                            '!flex': openTagRow === `${channel.deviceUID}-${channel.channelName}`,
                        }"
                    >
                        <button
                            v-if="hasRpm(channel)"
                            type="button"
                            class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                            v-tooltip.top="t('layout.shell.monitoringPanel.failAlert')"
                            @click.prevent="createFailAlert(channel)"
                        >
                            <svg-icon type="mdi" :path="mdiFanAlert" :size="16" />
                        </button>
                        <TagPopover
                            :device-u-i-d="channel.deviceUID"
                            :channel-name="channel.channelName"
                            @open="
                                (open: boolean) =>
                                    onTagOpen(`${channel.deviceUID}-${channel.channelName}`, open)
                            "
                        />
                        <span class="flex w-6 shrink-0 justify-center">
                            <CCColorPicker
                                :model-value="channelColor(channel.deviceUID, channel.channelName)"
                                :default-color="
                                    channelDefaultColor(channel.deviceUID, channel.channelName)
                                "
                                :size="1.25"
                                @update:model-value="(c: Color) => setChannelColor(channel, c)"
                                @reset="resetChannelColor(channel)"
                            />
                        </span>
                        <button
                            type="button"
                            class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                            v-tooltip.top="t('layout.shell.coolingPanel.unpin')"
                            @click.prevent="togglePin(channel)"
                        >
                            <svg-icon type="mdi" :path="mdiPinOff" :size="16" />
                        </button>
                        <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                            <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                        </span>
                    </div>
                </div>
            </VueDraggable>
            <UiSeparator class="my-1" />
        </template>

        <VueDraggable
            :force-auto-scroll-fallback="true"
            v-model="groups"
            handle=".device-drag-handle"
            :animation="150"
            class="flex flex-col gap-0.5"
            @end="persistDeviceOrder"
        >
            <div v-for="group in groups" :key="group.deviceUID" class="flex flex-col gap-0.5">
                <!-- The whole band is the link, as a channel row is, so the active
                     device reads the same way here as a selected channel does. The
                     label keeps the device color rather than turning accent: the
                     color is what identifies the device across every panel. -->
                <PanelHeader
                    class="group/device rounded-t-lg hover:bg-surface-hover"
                    :class="{
                        'bg-surface-hover': isRouteActive(deviceTarget(group.deviceUID)),
                    }"
                    :to="deviceTarget(group.deviceUID)"
                    :color="deviceColor(group.deviceUID) || 'rgb(var(--colors-text-color))'"
                >
                    <template #label>{{ deviceLabel(group.deviceUID) }}</template>
                    <!-- invisible, not hidden: the header must not change height on
                         hover. -mr-1 cancels the header's pr-2 down to the pr-1 the
                         rows below inset by, so the same p-1 handle puts the drag
                         glyph on the same column. -->
                    <span
                        class="invisible -mr-1 flex items-center gap-0.5 group-hover/device:visible group-has-[:focus-visible]/device:visible"
                        :class="{ '!visible': openColorDevice === group.deviceUID }"
                    >
                        <span class="flex w-6 shrink-0 justify-center">
                            <CCColorPicker
                                :model-value="devicePickerColor(group.deviceUID)"
                                :size="1.25"
                                @open="
                                    (open: boolean) =>
                                        (openColorDevice = open ? group.deviceUID : undefined)
                                "
                                @update:model-value="
                                    (c: Color) => setDeviceColor(group.deviceUID, c)
                                "
                            />
                        </span>
                        <!-- The rows' p-1 grab area, with -my-0.5 giving the extra
                             height back so the header does not grow for it. -->
                        <span
                            class="device-drag-handle -my-0.5 cursor-grab p-1 text-text-color-secondary"
                        >
                            <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                        </span>
                    </span>
                </PanelHeader>
                <VueDraggable
                    :force-auto-scroll-fallback="true"
                    v-model="group.channels"
                    handle=".drag-handle"
                    :animation="150"
                    class="flex flex-col gap-0.5"
                    @end="persistChannelOrder(group)"
                >
                    <div
                        v-for="channel in group.channels"
                        :key="channel.channelName"
                        class="group flex items-center rounded-lg hover:bg-surface-hover has-[:focus-visible]:bg-surface-hover has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent"
                        :class="{ 'bg-surface-hover': isRouteActive(channelTarget(channel)) }"
                    >
                        <RouterLink
                            :to="channelTarget(channel)"
                            class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none"
                            exact-active-class="!text-accent"
                        >
                            <svg-icon
                                type="mdi"
                                :path="mdiFan"
                                :size="18"
                                class="shrink-0"
                                :class="{
                                    'animate-spin-slow': channelSpins(
                                        'fan',
                                        liveFor(channel.deviceUID, channel.channelName),
                                        settingsStore.eyeCandy,
                                    ),
                                }"
                                :style="{
                                    color:
                                        channelColor(channel.deviceUID, channel.channelName) ||
                                        undefined,
                                }"
                            />
                            <span class="truncate">
                                {{ channelLabel(channel.deviceUID, channel.channelName) }}
                            </span>
                            <TagChips
                                :device-u-i-d="channel.deviceUID"
                                :channel-name="channel.channelName"
                            />
                            <ChannelHealthIcon
                                :device-u-i-d="channel.deviceUID"
                                :channel-name="channel.channelName"
                            />
                            <span
                                class="ml-auto flex items-baseline gap-1.5 whitespace-nowrap group-hover:hidden group-has-[:focus-visible]:hidden"
                                :class="{
                                    '!hidden':
                                        openTagRow ===
                                        `${channel.deviceUID}-${channel.channelName}`,
                                }"
                            >
                                <span
                                    v-if="
                                        liveFor(channel.deviceUID, channel.channelName)?.duty !=
                                        null
                                    "
                                    class="font-numeric tabular-nums text-text-color"
                                >
                                    {{ liveFor(channel.deviceUID, channel.channelName)?.duty }}%
                                </span>
                                <span
                                    v-if="
                                        liveFor(channel.deviceUID, channel.channelName)?.rpm != null
                                    "
                                    class="text-sm font-numeric tabular-nums text-text-color-secondary"
                                >
                                    {{ liveSpeedText(channel) }}
                                </span>
                            </span>
                        </RouterLink>
                        <div
                            class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex group-has-[[data-state=open]]:flex"
                            :class="{
                                '!flex':
                                    openTagRow === `${channel.deviceUID}-${channel.channelName}`,
                            }"
                        >
                            <button
                                v-if="hasRpm(channel)"
                                type="button"
                                class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                v-tooltip.top="t('layout.shell.monitoringPanel.failAlert')"
                                @click.prevent="createFailAlert(channel)"
                            >
                                <svg-icon type="mdi" :path="mdiFanAlert" :size="16" />
                            </button>
                            <TagPopover
                                :device-u-i-d="channel.deviceUID"
                                :channel-name="channel.channelName"
                                @open="
                                    (open: boolean) =>
                                        onTagOpen(
                                            `${channel.deviceUID}-${channel.channelName}`,
                                            open,
                                        )
                                "
                            />
                            <span class="flex w-6 shrink-0 justify-center">
                                <CCColorPicker
                                    :model-value="
                                        channelColor(channel.deviceUID, channel.channelName)
                                    "
                                    :default-color="
                                        channelDefaultColor(channel.deviceUID, channel.channelName)
                                    "
                                    :size="1.25"
                                    @update:model-value="(c: Color) => setChannelColor(channel, c)"
                                    @reset="resetChannelColor(channel)"
                                />
                            </span>
                            <button
                                type="button"
                                class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                v-tooltip.top="
                                    isPinned(channel)
                                        ? t('layout.shell.coolingPanel.unpin')
                                        : t('layout.shell.coolingPanel.pin')
                                "
                                @click.prevent="togglePin(channel)"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="isPinned(channel) ? mdiPinOff : mdiPinOutline"
                                    :size="16"
                                />
                            </button>
                            <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                                <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                            </span>
                        </div>
                    </div>
                </VueDraggable>
            </div>
        </VueDraggable>

        <!-- Without this the panel would show a lone Functions header and no
             hint that the missing fans are the point. -->
        <div v-if="groups.length === 0" class="px-3 py-2">
            <HardwareHelpLine />
        </div>

        <UiSeparator class="my-1" />
        <PanelHeader :label="t('layout.shell.coolingPanel.library')" />
        <LibraryList
            kind="profiles"
            :label="t('layout.shell.coolingPanel.profiles')"
            :add-tooltip="t('layout.menu.tooltips.addProfile')"
            :icon="mdiChartMultiple"
            route-name="profiles"
            param-name="profileUID"
            :entities="profileLinks"
            @add="openProfileWizard"
            @reorder="sortProfiles"
        >
            <template #badge="{ uid }">
                <UiTooltip v-if="isProfileUnhealthy(uid)" :text="profileTooltip(uid)">
                    <svg-icon type="mdi" :path="mdiAlert" :size="14" class="shrink-0 text-error" />
                </UiTooltip>
            </template>
        </LibraryList>
        <LibraryList
            kind="functions"
            :label="t('layout.shell.coolingPanel.functions')"
            :add-tooltip="t('layout.menu.tooltips.addFunction')"
            :icon="mdiFunction"
            route-name="functions"
            param-name="functionUID"
            :entities="functionLinks"
            class="pt-1"
            @add="openFunctionWizard"
            @reorder="sortFunctions"
        />
    </div>
</template>
