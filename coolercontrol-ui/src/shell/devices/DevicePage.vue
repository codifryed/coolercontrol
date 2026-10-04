<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiPlus, mdiToggleSwitchOffOutline } from '@mdi/js'
import HelpIcon from '@/components/info/HelpIcon.vue'
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { type Color, DeviceType, getDeviceTypeDisplayName, type UID } from '@/models/Device.ts'
import { getDriverTypeDisplayName } from '@/models/DeviceInfo.ts'
import CCColorPicker from '@/components/CCColorPicker.vue'
import EntityTitleRename from '@/components/EntityTitleRename.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useDeviceActions } from '@/composables/useDeviceActions.ts'
import {
    customSensorNames,
    type DeviceSensorLink,
    deviceSensorLinks,
} from '@/shell/devices/devices.ts'
import UiButton from '@/shell/ui/UiButton.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiSeparator from '@/shell/ui/UiSeparator.vue'
import UiSwitch from '@/shell/ui/UiSwitch.vue'

const props = defineProps<{ deviceUID: UID }>()
const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const colorStore = useThemeColorsStore()
const deviceActions = useDeviceActions()
const router = useRouter()

const device = computed(() =>
    [...deviceStore.allDevices()].find((dev) => dev.uid === props.deviceUID),
)
const ccSetting = computed(() => settingsStore.ccDeviceSettings.get(props.deviceUID))
const isDisabledDevice = computed(() => device.value == null && ccSetting.value?.disable === true)

const uiSettings = computed(() => settingsStore.allUIDeviceSettings.get(props.deviceUID))
const deviceLabel = computed(
    () => uiSettings.value?.name ?? ccSetting.value?.name ?? props.deviceUID,
)
// The color picker needs a concrete color; var() strings are not parseable.
const deviceColor = computed(
    () => uiSettings.value?.userColor || `rgb(${colorStore.themeColors.text_color})`,
)

const saveNameFunction = async (newName: string): Promise<boolean> =>
    settingsStore.saveDeviceName(props.deviceUID, newName)
const defaultName = computed(() => settingsStore.defaultDeviceName(props.deviceUID))

const setDeviceColor = (newColor: Color): void => {
    if (uiSettings.value != null) uiSettings.value.userColor = newColor
}

// Info rows, mirroring the old device-info popover.
const infoRows = computed((): Array<[string, string]> => {
    const dev = device.value
    if (dev == null) return []
    const rows: Array<[string, string]> = [
        [t('components.deviceInfo.systemName'), dev.name],
        [t('components.deviceInfo.deviceType'), getDeviceTypeDisplayName(dev.type)],
        [t('components.deviceInfo.deviceUID'), dev.uid],
    ]
    if (dev.lc_info?.firmware_version) {
        rows.push([t('components.deviceInfo.firmwareVersion'), dev.lc_info.firmware_version])
    }
    if (dev.info?.model) rows.push([t('components.deviceInfo.model'), dev.info.model])
    if (dev.info?.driver_info.name) {
        rows.push([t('components.deviceInfo.driverName'), dev.info.driver_info.name])
    }
    if (dev.info != null) {
        rows.push([
            t('components.deviceInfo.driverType'),
            getDriverTypeDisplayName(dev.info.driver_info.drv_type),
        ])
    }
    if (dev.info?.driver_info.version) {
        rows.push([t('components.deviceInfo.driverVersion'), dev.info.driver_info.version])
    }
    if (dev.info != null && dev.info.driver_info.locations.length > 0) {
        rows.push([t('components.deviceInfo.locations'), dev.info.driver_info.locations.join('\n')])
    }
    return rows
})

// Hardware settings, ported from the extension-settings popover.
const isLiquidctl = computed(() => device.value?.type === DeviceType.LIQUIDCTL)
const isCustomSensors = computed(() => device.value?.type === DeviceType.CUSTOM_SENSORS)
const sensorNames = computed(() => (device.value != null ? customSensorNames(device.value) : []))
const sensorDotColor = (channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(props.deviceUID)?.sensorsAndChannels.get(channelName)
        ?.color || 'rgb(var(--colors-text-color))'
const hasHwmonDriver = computed(
    () => device.value?.info?.driver_info.locations.find((loc) => loc.includes('hwmon')) != null,
)
const isAmdGpuWithOverdrive = computed(() => device.value?.info?.amd_gpu_overdrive != null)
const amdOverdriveEnabled = computed(() => device.value?.info?.amd_gpu_overdrive === true)
const amdOverdriveEnabling = ref(false)
const isThinkPad = computed(() => device.value?.info?.thinkpad_fan_control != null)

const deviceExtensions = computed(() => ccSetting.value?.extensions)
const directAccess = ref(deviceExtensions.value?.direct_access ?? false)
const useHwmon = ref(false)
const delayMillis = ref(deviceExtensions.value?.delay_millis ?? 0)

const onDirectAccessChange = (value: boolean): void => {
    if (deviceExtensions.value == null || value === deviceExtensions.value.direct_access) return
    deviceActions.setDirectAccess(props.deviceUID, value, () => {
        directAccess.value = deviceExtensions.value!.direct_access
    })
}

const onUseHwmonChange = (value: boolean): void => {
    if (!value) return
    deviceActions.switchToHwmon(props.deviceUID, () => {
        useHwmon.value = false
    })
}

// Command delay changes require a daemon restart, so commit once the value
// settles (debounced) instead of on every keystroke or step. Track the last
// committed value so a clamp/round write-back cannot retrigger a second commit.
let lastDelayIntent = deviceExtensions.value?.delay_millis ?? 0
const commitDelay = (): void => {
    const value = Math.min(250, Math.max(0, Math.round(delayMillis.value ?? 0)))
    if (delayMillis.value !== value) delayMillis.value = value
    if (value === lastDelayIntent) return
    lastDelayIntent = value
    if (deviceExtensions.value == null || value === deviceExtensions.value.delay_millis) return
    deviceActions.setDelayMillis(props.deviceUID, value, () => {
        delayMillis.value = deviceExtensions.value!.delay_millis
        lastDelayIntent = delayMillis.value
    })
}
let delayCommitTimer: ReturnType<typeof setTimeout> | undefined
watch(delayMillis, () => {
    clearTimeout(delayCommitTimer)
    delayCommitTimer = setTimeout(commitDelay, 500)
})
onBeforeUnmount(() => clearTimeout(delayCommitTimer))

const onEnableOverdrive = async (): Promise<void> => {
    amdOverdriveEnabling.value = true
    await deviceActions.enableAmdOverdrive()
    amdOverdriveEnabling.value = false
}

const thinkPadFanControl = computed({
    get: () => settingsStore.thinkPadFanControlEnabled,
    set: (value: boolean) => deviceActions.applyThinkPadFanControl(value),
})
const thinkPadFullSpeed = computed({
    // persisted automatically through the global CC settings watcher
    get: () => settingsStore.ccSettings.thinkpad_full_speed,
    set: (value: boolean) => (settingsStore.ccSettings.thinkpad_full_speed = value),
})

const sensorLabel = (channelName: string): string =>
    uiSettings.value?.sensorsAndChannels.get(channelName)?.name ??
    ccSetting.value?.channel_settings.get(channelName)?.label ??
    channelName

// One complete list of the device's channels and sensors, each linking to the
// page where it is viewed or controlled.
const sensorLinks = computed((): DeviceSensorLink[] =>
    device.value != null ? deviceSensorLinks(device.value, settingsStore.channelUnit) : [],
)
const sensorLinkTarget = (
    link: DeviceSensorLink,
): { name: string; params: Record<string, string> } => {
    const params = { deviceUID: props.deviceUID, channelName: link.channelName }
    switch (link.kind) {
        case 'cooling':
            return { name: 'cooling-channel', params }
        case 'lighting':
            return { name: 'device-lighting', params }
        case 'lcd':
            return { name: 'device-lcd', params }
        default:
            return { name: 'monitoring-sensor', params }
    }
}
const sensorDestLabel = (kind: DeviceSensorLink['kind']): string =>
    t(`layout.shell.sensorDest.${kind}`)

// Enabling/disabling devices and sensors happens in one place: the Manage
// Sensors editor, deep-linked to this device.
const openManageSensors = (): void => {
    router.push({ name: 'devices-manage-sensors', query: { device: props.deviceUID } })
}
const enableDevice = openManageSensors
</script>

<template>
    <div class="flex h-full flex-col overflow-y-auto p-4">
        <!-- Disabled device: minimal page with re-enable. -->
        <template v-if="isDisabledDevice">
            <h1 class="text-xl font-semibold text-text-color">{{ deviceLabel }}</h1>
            <p class="mt-2 text-base text-text-color-secondary">
                {{ t('layout.shell.devicesPage.deviceDisabled') }}
            </p>
            <UiButton class="mt-4 w-fit" variant="outline" @click="enableDevice">
                {{ t('layout.shell.devicesPage.enableDevice') }}
            </UiButton>
        </template>

        <template v-else-if="device != null">
            <div class="flex items-center gap-2">
                <EntityTitleRename
                    :current-name="deviceLabel"
                    :fallback-name="defaultName"
                    :save-name-function="saveNameFunction"
                />
                <CCColorPicker
                    :model-value="deviceColor"
                    :size="1.5"
                    @update:model-value="setDeviceColor"
                />
            </div>

            <button
                v-if="!isCustomSensors"
                type="button"
                class="mt-3 inline-flex min-h-10 w-fit shrink-0 items-center gap-2 rounded-lg border border-error/40 bg-control px-4 py-1.5 text-base font-medium text-text-color outline-none transition-colors hover:bg-error/10 focus-visible:ring-2 focus-visible:ring-accent"
                @click="openManageSensors"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiToggleSwitchOffOutline"
                    :size="18"
                    class="text-text-color-secondary"
                />
                {{ t('layout.shell.devicesPage.disableUnusedSensors') }}
            </button>

            <!-- Device details -->
            <h2 class="pb-2 pt-4 text-xs uppercase text-text-color-secondary">
                {{ t('components.deviceInfo.details') }}
            </h2>
            <div class="w-fit rounded-lg border border-border-one bg-bg-two px-4 py-3">
                <div class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1 text-base">
                    <template v-for="[label, value] in infoRows" :key="label">
                        <span class="text-text-color-secondary">{{ label }}</span>
                        <span class="whitespace-pre-line break-all text-text-color">
                            {{ value }}
                        </span>
                    </template>
                </div>
            </div>

            <!-- Custom sensors are created and edited from this device page. -->
            <template v-if="isCustomSensors">
                <h2 class="pb-2 pt-6 text-xs uppercase text-text-color-secondary">
                    {{ t('layout.menu.customSensors') }}
                </h2>
                <div
                    class="flex w-fit min-w-96 flex-col rounded-lg border border-border-one bg-bg-two p-2"
                >
                    <RouterLink
                        v-for="sensorName in sensorNames"
                        :key="sensorName"
                        :to="{
                            name: 'device-custom-sensor',
                            params: { customSensorID: sensorName },
                        }"
                        class="flex items-center gap-2 rounded-lg px-2 py-1.5 text-text-color outline-none hover:bg-surface-hover focus:ring-2 focus:ring-accent"
                    >
                        <span
                            class="h-2 w-2 shrink-0 rounded-full"
                            :style="{ backgroundColor: sensorDotColor(sensorName) }"
                        />
                        <span class="truncate">{{ sensorLabel(sensorName) }}</span>
                    </RouterLink>
                    <RouterLink
                        :to="{ name: 'device-custom-sensor-new' }"
                        class="flex items-center gap-2 rounded-lg px-2 py-1.5 text-text-color-secondary outline-none hover:bg-surface-hover hover:text-text-color focus:ring-2 focus:ring-accent"
                    >
                        <svg-icon type="mdi" :path="mdiPlus" :size="14" class="shrink-0" />
                        {{ t('layout.menu.tooltips.addCustomSensor') }}
                    </RouterLink>
                </div>
            </template>
            <!-- Hardware settings -->
            <template v-if="!isCustomSensors">
                <h2 class="pb-2 pt-6 text-xs uppercase text-text-color-secondary">
                    {{ t('components.deviceExtensionSettings.title') }}
                </h2>
                <div
                    class="flex w-fit min-w-96 flex-col rounded-lg border border-border-one bg-bg-two"
                >
                    <div
                        v-if="isLiquidctl"
                        class="flex items-center justify-between gap-8 px-4 py-3"
                    >
                        <span class="flex items-center gap-2 text-base text-text-color">
                            <HelpIcon
                                :text="t('components.deviceExtensionSettings.directAccessDesc')"
                                :size="1.125"
                            />
                            {{ t('components.deviceExtensionSettings.directAccess') }}
                        </span>
                        <UiSwitch
                            v-model="directAccess"
                            :disabled="!hasHwmonDriver"
                            @update:model-value="onDirectAccessChange"
                        />
                    </div>
                    <template v-if="isLiquidctl">
                        <UiSeparator />
                        <div class="flex items-center justify-between gap-8 px-4 py-3">
                            <span class="flex items-center gap-2 text-base text-text-color">
                                <HelpIcon
                                    :text="t('components.deviceExtensionSettings.useHwmonDesc')"
                                    :size="1.125"
                                />
                                {{ t('components.deviceExtensionSettings.useHwmon') }}
                            </span>
                            <UiSwitch
                                v-model="useHwmon"
                                :disabled="!hasHwmonDriver"
                                @update:model-value="onUseHwmonChange"
                            />
                        </div>
                    </template>
                    <template v-if="isAmdGpuWithOverdrive">
                        <UiSeparator v-if="isLiquidctl" />
                        <div class="flex items-center justify-between gap-8 px-4 py-3">
                            <span class="flex items-center gap-2 text-base text-text-color">
                                <HelpIcon
                                    :text="t('components.deviceExtensionSettings.overdriveDesc')"
                                    :size="1.125"
                                />
                                {{ t('components.deviceExtensionSettings.overdrive') }}
                            </span>
                            <span v-if="amdOverdriveEnabled" class="text-base text-success">
                                {{ t('components.deviceExtensionSettings.overdriveActive') }}
                            </span>
                            <UiButton
                                v-else
                                size="sm"
                                variant="outline"
                                :disabled="amdOverdriveEnabling"
                                @click="onEnableOverdrive"
                            >
                                {{ t('components.deviceExtensionSettings.overdriveEnable') }}
                            </UiButton>
                        </div>
                    </template>
                    <template v-if="isThinkPad">
                        <UiSeparator v-if="isLiquidctl || isAmdGpuWithOverdrive" />
                        <div class="flex items-center justify-between gap-8 px-4 py-3">
                            <span class="flex items-center gap-2 text-base text-text-color">
                                <HelpIcon
                                    :text="
                                        t(
                                            'components.deviceExtensionSettings.thinkPadFanControlDesc',
                                        )
                                    "
                                    :size="1.125"
                                />
                                {{ t('components.deviceExtensionSettings.thinkPadFanControl') }}
                            </span>
                            <UiSwitch v-model="thinkPadFanControl" />
                        </div>
                        <UiSeparator />
                        <div class="flex items-center justify-between gap-8 px-4 py-3">
                            <span class="flex items-center gap-2 text-base text-text-color">
                                <HelpIcon
                                    :text="
                                        t(
                                            'components.deviceExtensionSettings.thinkPadFullSpeedDesc',
                                        )
                                    "
                                    :size="1.125"
                                />
                                {{ t('components.deviceExtensionSettings.thinkPadFullSpeed') }}
                            </span>
                            <UiSwitch v-model="thinkPadFullSpeed" />
                        </div>
                    </template>
                    <UiSeparator v-if="isLiquidctl || isAmdGpuWithOverdrive || isThinkPad" />
                    <div class="flex items-center justify-between gap-8 px-4 py-3">
                        <span class="flex items-center gap-2 text-base text-text-color">
                            <HelpIcon
                                :text="t('components.deviceExtensionSettings.commandDelayDesc')"
                                :size="1.125"
                            />
                            {{ t('components.deviceExtensionSettings.commandDelay') }}
                        </span>
                        <UiNumberInput
                            v-model="delayMillis"
                            :min="0"
                            :max="250"
                            :step="10"
                            suffix="ms"
                        />
                    </div>
                </div>
            </template>
            <!-- Complete sensor/channel list; each links to where it is viewed or controlled. -->
            <template v-if="!isCustomSensors && sensorLinks.length > 0">
                <h2 class="pb-2 pt-6 text-xs uppercase text-text-color-secondary">
                    {{ t('layout.shell.devicesPage.sensors') }}
                </h2>
                <div
                    class="flex w-fit min-w-96 flex-col rounded-lg border border-border-one bg-bg-two p-2"
                >
                    <RouterLink
                        v-for="link in sensorLinks"
                        :key="`${link.kind}-${link.channelName}`"
                        :to="sensorLinkTarget(link)"
                        class="flex items-center gap-2 rounded-lg px-2 py-1.5 text-base text-text-color outline-none hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
                    >
                        <span
                            class="h-2 w-2 shrink-0 rounded-full"
                            :style="{ backgroundColor: sensorDotColor(link.channelName) }"
                        />
                        <span class="truncate">{{ sensorLabel(link.channelName) }}</span>
                        <span class="ml-auto shrink-0 pl-3 text-xs text-text-color-secondary">
                            {{ sensorDestLabel(link.kind) }} &rsaquo;
                        </span>
                    </RouterLink>
                </div>
            </template>
            <div class="pb-8" />
        </template>
    </div>
</template>
