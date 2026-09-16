<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiAlert,
    mdiLanDisconnect,
    mdiDragVertical,
    mdiLightbulbOutline,
    mdiPinOff,
    mdiPinOutline,
    mdiPlus,
    mdiTelevision,
    mdiThermometer,
} from '@mdi/js'
import { VueDraggable } from 'vue-draggable-plus'
import { computed, ref, watchEffect } from 'vue'
import { useI18n } from 'vue-i18n'
import {
    type Color,
    type Device,
    DeviceType,
    getDeviceTypeDisplayName,
    type UID,
} from '@/models/Device.ts'
import CCColorPicker from '@/components/CCColorPicker.vue'
import PanelHeader from '@/shell/PanelHeader.vue'
import TagPopover from '@/shell/monitoring/TagPopover.vue'
import UiTooltip from '@/shell/ui/UiTooltip.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { customSensorNames, deviceChannelLinks, hardwareDevices } from '@/shell/devices/devices.ts'
import { deviceTypeIcon } from '@/shell/deviceIcon.ts'
import { pinId } from '@/shell/cooling/channels.ts'
import { setDeviceChildrenSubset, setTopLevelOrder } from '@/shell/panelOrder.ts'
import { useRouteActive } from '@/shell/routeActive.ts'
import type { RouteLocationRaw } from 'vue-router'
import { useDeviceHealth } from '@/composables/useDeviceHealth.ts'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const colorStore = useThemeColorsStore()

// One flat drag-sortable list; the device type shows as a row suffix.
const devicesList = ref<Device[]>([])
watchEffect(() => {
    devicesList.value = hardwareDevices(deviceStore.allDevices())
})

const persistDeviceOrder = (): void => {
    const orderedUids = devicesList.value.map((device) => device.uid)
    settingsStore.menuOrder = setTopLevelOrder(settingsStore.menuOrder, orderedUids)
    deviceStore.reSortDevicesByMenuOrder()
}

// Custom sensor children are drag-sortable too.
const sensorNamesByDevice = ref<Map<UID, string[]>>(new Map())
watchEffect(() => {
    const map = new Map<UID, string[]>()
    for (const device of devicesList.value) {
        if (device.type === DeviceType.CUSTOM_SENSORS) {
            map.set(device.uid, customSensorNames(device))
        }
    }
    sensorNamesByDevice.value = map
})

const persistSensorOrder = (deviceUID: UID): void => {
    const names = sensorNamesByDevice.value.get(deviceUID) ?? []
    const ids = names.map((name) => pinId(deviceUID, name))
    settingsStore.menuOrder = setDeviceChildrenSubset(settingsStore.menuOrder, deviceUID, ids, ids)
    deviceStore.reSortDevicesByMenuOrder()
}

const disabledDevices = computed(() =>
    [...settingsStore.ccDeviceSettings.values()].filter((setting) => setting.disable),
)

const deviceLabel = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.name ?? deviceUID

const deviceColor = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.userColor ?? ''

// Devices without a custom color fall back to the theme text color. The dot
// can use the CSS variable directly; the picker needs a concrete color value
// (its conversion helpers cannot parse var() strings).
const dotColor = (deviceUID: UID): string =>
    deviceColor(deviceUID) || 'rgb(var(--colors-text-color))'
const pickerColor = (deviceUID: UID): string =>
    deviceColor(deviceUID) || `rgb(${colorStore.themeColors.text_color})`

const sensorDotColor = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.color ||
    'rgb(var(--colors-text-color))'
const sensorPickerColor = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.color ||
    `rgb(${colorStore.themeColors.text_color})`

const channelLabel = (deviceUID: UID, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.name ??
    channelName

// The virtual CustomSensors device shows health on the affected sensor rows
// instead of the device row.
const { isDeviceUnreachable, isDeviceUnhealthy: isUnhealthyUid, healthTooltip } = useDeviceHealth()

const isDeviceUnhealthy = (device: Device): boolean =>
    device.type !== DeviceType.CUSTOM_SENSORS && isUnhealthyUid(device.uid)

const isChannelUnhealthy = (deviceUID: UID, channelName: string): boolean =>
    settingsStore.healthFailsafe.some(
        (ref) => ref.device_uid === deviceUID && ref.name === channelName,
    )

const setDeviceColor = (deviceUID: UID, newColor: Color): void => {
    const setting = settingsStore.allUIDeviceSettings.get(deviceUID)
    if (setting != null) setting.userColor = newColor
}

const setSensorColor = (deviceUID: UID, channelName: string, newColor: Color): void => {
    const setting = settingsStore.allUIDeviceSettings
        .get(deviceUID)
        ?.sensorsAndChannels.get(channelName)
    if (setting != null) setting.userColor = newColor
}
const sensorDefaultColor = (deviceUID: UID, channelName: string): Color | undefined =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)
        ?.defaultColor
// Reset clears the user override so the non-user-defined color applies again.
const resetSensorColor = (deviceUID: UID, channelName: string): void => {
    const setting = settingsStore.allUIDeviceSettings
        .get(deviceUID)
        ?.sensorsAndChannels.get(channelName)
    if (setting != null) setting.userColor = undefined
}

const isPinned = (deviceUID: UID, channelName: string): boolean =>
    settingsStore.pinnedIds.includes(pinId(deviceUID, channelName))

const togglePin = (deviceUID: UID, channelName: string): void => {
    const id = pinId(deviceUID, channelName)
    settingsStore.pinnedIds = settingsStore.pinnedIds.includes(id)
        ? settingsStore.pinnedIds.filter((pinned) => pinned !== id)
        : [...settingsStore.pinnedIds, id]
}

// Keeps a row's hover actions visible while its tag popover is open.
const openTagRow = ref<string | null>(null)
const onTagOpen = (rowKey: string, open: boolean): void => {
    openTagRow.value = open ? rowKey : null
}

// The row wrapper needs the same target its link uses, so both read it here.
const deviceTarget = (deviceUID: UID): RouteLocationRaw => ({
    name: 'devices-device',
    params: { deviceUID },
})
const customSensorTarget = (customSensorID: string): RouteLocationRaw => ({
    name: 'device-custom-sensor',
    params: { customSensorID },
})
const isRouteActive = useRouteActive()
</script>

<template>
    <div class="flex flex-col gap-0.5 p-2 pb-24 text-base">
        <VueDraggable
            :force-auto-scroll-fallback="true"
            v-model="devicesList"
            handle=".drag-handle"
            :animation="150"
            class="flex flex-col gap-0.5"
            @end="persistDeviceOrder"
        >
            <div v-for="device in devicesList" :key="device.uid" class="flex flex-col gap-0.5">
                <div
                    class="group flex items-center rounded-lg hover:bg-surface-hover has-[:focus-visible]:bg-surface-hover has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent"
                    :class="{ 'bg-surface-hover': isRouteActive(deviceTarget(device.uid)) }"
                >
                    <RouterLink
                        :to="deviceTarget(device.uid)"
                        class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none"
                        exact-active-class="!text-accent"
                    >
                        <svg-icon
                            type="mdi"
                            :path="deviceTypeIcon(device.type)"
                            :size="18"
                            class="shrink-0"
                            :style="{ color: dotColor(device.uid) }"
                        />
                        <span class="truncate">{{ deviceLabel(device.uid) }}</span>
                        <span
                            v-if="getDeviceTypeDisplayName(device.type) !== deviceLabel(device.uid)"
                            class="shrink-0 text-xs text-text-color-secondary"
                        >
                            {{ getDeviceTypeDisplayName(device.type) }}
                        </span>
                        <UiTooltip
                            v-if="isDeviceUnhealthy(device)"
                            :text="healthTooltip(device.uid)"
                        >
                            <svg-icon
                                type="mdi"
                                :path="
                                    isDeviceUnreachable(device.uid) ? mdiLanDisconnect : mdiAlert
                                "
                                :size="14"
                                class="shrink-0 text-error"
                            />
                        </UiTooltip>
                    </RouterLink>
                    <div
                        class="ml-auto hidden items-center gap-0.5 group-hover:flex group-has-[:focus-visible]:flex group-has-[[data-state=open]]:flex"
                        :class="{ 'pr-1': device.type !== DeviceType.CUSTOM_SENSORS }"
                    >
                        <span class="flex w-6 shrink-0 justify-center">
                            <CCColorPicker
                                :model-value="pickerColor(device.uid)"
                                :size="1.25"
                                @update:model-value="(c: Color) => setDeviceColor(device.uid, c)"
                            />
                        </span>
                        <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                            <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                        </span>
                    </div>
                    <!-- Always shown, like the add in the other panel headers. -->
                    <RouterLink
                        v-if="device.type === DeviceType.CUSTOM_SENSORS"
                        :to="{ name: 'device-custom-sensor-new' }"
                        class="ml-0.5 mr-1 rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                        v-tooltip.top="t('layout.menu.tooltips.addCustomSensor')"
                    >
                        <svg-icon type="mdi" :path="mdiPlus" :size="16" />
                    </RouterLink>
                </div>
                <RouterLink
                    v-for="link in deviceChannelLinks(device)"
                    :key="`${link.kind}-${link.channelName}`"
                    :to="{
                        name: link.kind === 'lighting' ? 'device-lighting' : 'device-lcd',
                        params: { deviceUID: link.deviceUID, channelName: link.channelName },
                    }"
                    class="flex items-center gap-2 rounded-lg py-1 pl-6 pr-2 text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                    exact-active-class="bg-surface-hover !text-accent"
                >
                    <svg-icon
                        type="mdi"
                        :path="link.kind === 'lighting' ? mdiLightbulbOutline : mdiTelevision"
                        :size="18"
                        class="shrink-0 text-text-color-secondary"
                    />
                    <span class="truncate">{{
                        channelLabel(link.deviceUID, link.channelName)
                    }}</span>
                </RouterLink>
                <template v-if="device.type === DeviceType.CUSTOM_SENSORS">
                    <VueDraggable
                        :force-auto-scroll-fallback="true"
                        :model-value="sensorNamesByDevice.get(device.uid) ?? []"
                        handle=".drag-handle"
                        :animation="150"
                        class="flex flex-col gap-0.5"
                        @update:model-value="
                            (names: string[]) => sensorNamesByDevice.set(device.uid, names)
                        "
                        @end="persistSensorOrder(device.uid)"
                    >
                        <div
                            v-for="sensorName in sensorNamesByDevice.get(device.uid) ?? []"
                            :key="`custom-${sensorName}`"
                            class="group flex items-center rounded-lg hover:bg-surface-hover has-[:focus-visible]:bg-surface-hover has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent"
                            :class="{
                                'bg-surface-hover': isRouteActive(customSensorTarget(sensorName)),
                            }"
                        >
                            <RouterLink
                                :to="customSensorTarget(sensorName)"
                                class="flex min-w-0 flex-1 items-center gap-2 rounded-lg py-1 pl-6 pr-2 text-text-color outline-none"
                                exact-active-class="!text-accent"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="mdiThermometer"
                                    :size="18"
                                    class="shrink-0"
                                    :style="{
                                        color: sensorDotColor(device.uid, sensorName) || undefined,
                                    }"
                                />
                                <span class="truncate">
                                    {{ channelLabel(device.uid, sensorName) }}
                                </span>
                                <UiTooltip
                                    v-if="isChannelUnhealthy(device.uid, sensorName)"
                                    :text="healthTooltip(device.uid, sensorName)"
                                >
                                    <svg-icon
                                        type="mdi"
                                        :path="mdiAlert"
                                        :size="14"
                                        class="shrink-0 text-error"
                                    />
                                </UiTooltip>
                            </RouterLink>
                            <div
                                class="ml-auto hidden items-center gap-0.5 pr-1 group-hover:flex group-has-[:focus-visible]:flex group-has-[[data-state=open]]:flex"
                                :class="{
                                    '!flex': openTagRow === `${device.uid}-${sensorName}`,
                                }"
                            >
                                <TagPopover
                                    :device-u-i-d="device.uid"
                                    :channel-name="sensorName"
                                    @open="
                                        (open: boolean) =>
                                            onTagOpen(`${device.uid}-${sensorName}`, open)
                                    "
                                />
                                <span class="flex w-6 shrink-0 justify-center">
                                    <CCColorPicker
                                        :model-value="sensorPickerColor(device.uid, sensorName)"
                                        :default-color="sensorDefaultColor(device.uid, sensorName)"
                                        :size="1.25"
                                        @update:model-value="
                                            (c: Color) => setSensorColor(device.uid, sensorName, c)
                                        "
                                        @reset="resetSensorColor(device.uid, sensorName)"
                                    />
                                </span>
                                <button
                                    type="button"
                                    class="rounded p-1 text-text-color-secondary outline-none hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                                    v-tooltip.top="
                                        isPinned(device.uid, sensorName)
                                            ? t('layout.shell.coolingPanel.unpin')
                                            : t('layout.shell.coolingPanel.pin')
                                    "
                                    @click.prevent="togglePin(device.uid, sensorName)"
                                >
                                    <svg-icon
                                        type="mdi"
                                        :path="
                                            isPinned(device.uid, sensorName)
                                                ? mdiPinOff
                                                : mdiPinOutline
                                        "
                                        :size="16"
                                    />
                                </button>
                                <span class="drag-handle cursor-grab p-1 text-text-color-secondary">
                                    <svg-icon type="mdi" :path="mdiDragVertical" :size="16" />
                                </span>
                            </div>
                        </div>
                    </VueDraggable>
                    <RouterLink
                        :to="{ name: 'device-custom-sensor-new' }"
                        class="flex items-center gap-2 rounded-lg py-1 pl-6 pr-2 text-text-color-secondary outline-none hover:bg-surface-hover hover:text-text-color focus-visible:ring-2 focus-visible:ring-accent"
                    >
                        <svg-icon type="mdi" :path="mdiPlus" :size="18" class="shrink-0" />
                        <span class="truncate">
                            {{ t('layout.menu.tooltips.addCustomSensor') }}
                        </span>
                    </RouterLink>
                </template>
            </div>
        </VueDraggable>

        <template v-if="disabledDevices.length > 0">
            <PanelHeader :label="t('layout.shell.devicesPanel.disabled')" />
            <RouterLink
                v-for="setting in disabledDevices"
                :key="`disabled-${setting.uid}`"
                :to="{ name: 'devices-device', params: { deviceUID: setting.uid } }"
                class="block truncate rounded-lg px-2 py-1.5 text-text-color-secondary outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                exact-active-class="bg-surface-hover !text-accent"
            >
                {{ setting.name }}
            </RouterLink>
        </template>
    </div>
</template>
