<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2025  Guy Boldon and contributors
  -
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  -
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  -
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { mdiAlertOutline, mdiChartLine, mdiCogs, mdiInformationSlabCircleOutline } from '@mdi/js'
import ProgressBar from 'primevue/progressbar'
import { PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useCalibrationStore } from '@/stores/CalibrationStore.ts'
import { computed, defineAsyncComponent, nextTick, ref, Ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { UID } from '@/models/Device.ts'
import { ChannelExtensionNames } from '@/models/SpeedOptions.ts'
import { Profile, ProfileType } from '@/models/Profile.ts'
import { CCChannelSettings, ChannelExtensions } from '@/models/CCSettings.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import { useToast } from 'primevue/usetoast'
import { useDialog } from 'primevue/usedialog'

const props = defineProps<{
    deviceUID: UID
    channelName: string
    chosenProfile?: Profile
}>()
const emit = defineEmits<{
    (e: 'change'): void
}>()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const calibrationStore = useCalibrationStore()
const toast = useToast()
const dialog = useDialog()
const { t } = useI18n()
const calibrationCurveDialog = defineAsyncComponent(
    () => import('@/components/CalibrationCurveDialog.vue'),
)
const isPopupOpen = ref(false)
const hwFanCurve: Ref<boolean> = ref(false)
const currentChannelExtension: Ref<ChannelExtensionNames | undefined> = ref()

// --- Calibration state and helpers ---------------------------------

const calibrationEligible = computed((): boolean => {
    for (const device of deviceStore.allDevices()) {
        if (device.uid !== props.deviceUID) continue
        const channelInfo = device.info?.channels.get(props.channelName)
        if (channelInfo?.speed_options?.fixed_enabled) {
            return true
        }
    }
    return false
})

const calibrationStatus = computed(() =>
    calibrationStore.statusFor(props.deviceUID, props.channelName),
)

const calibrationPhase = computed(() => calibrationStatus.value?.phase ?? ('not_started' as const))

const calibrationStatusText = computed((): string => {
    const status = calibrationStatus.value
    if (status == null || status.phase === 'not_started') {
        return t('components.channelExtensionSettings.calibration.statusNotCalibrated')
    }
    if (status.phase === 'in_progress') {
        const stage = stageLabel(status.stage)
        return t('components.channelExtensionSettings.calibration.statusInProgress', {
            stage,
            percent: status.percent,
        })
    }
    if (status.phase === 'completed') {
        return status.calibration.curve_kind === 'Stepped'
            ? t('components.channelExtensionSettings.calibration.statusCompletedStepped')
            : t('components.channelExtensionSettings.calibration.statusCompleted')
    }
    return t('components.channelExtensionSettings.calibration.statusFailed', {
        message: status.message,
    })
})

function stageLabel(stage: 'preflight' | 'up_sweep' | 'down_sweep' | 'finalizing'): string {
    switch (stage) {
        case 'preflight':
            return t('components.channelExtensionSettings.calibration.stagePreflight')
        case 'up_sweep':
            return t('components.channelExtensionSettings.calibration.stageUpSweep')
        case 'down_sweep':
            return t('components.channelExtensionSettings.calibration.stageDownSweep')
        case 'finalizing':
            return t('components.channelExtensionSettings.calibration.stageFinalizing')
    }
}

async function onStartCalibration(): Promise<void> {
    const result = await calibrationStore.startCalibration(props.deviceUID, props.channelName)
    if (result instanceof ErrorResponse) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: result.error || t('components.channelExtensionSettings.calibration.startError'),
            life: 6000,
        })
    }
}

async function onCancelCalibration(): Promise<void> {
    const result = await calibrationStore.cancelCalibration(props.deviceUID, props.channelName)
    if (result instanceof ErrorResponse) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail:
                result.error || t('components.channelExtensionSettings.calibration.cancelError'),
            life: 6000,
        })
    }
}

function onViewCurve(): void {
    const status = calibrationStatus.value
    const calibration = status?.phase === 'completed' ? status.calibration : undefined
    dialog.open(calibrationCurveDialog, {
        props: {
            header: t('components.calibrationCurve.dialogTitle'),
            position: 'center',
            modal: true,
            dismissableMask: true,
            style: { width: '80vw', maxWidth: '60rem' },
            breakpoints: { '1199px': '90vw', '767px': '95vw' },
        },
        data: {
            deviceUID: props.deviceUID,
            channelName: props.channelName,
            calibration,
        },
    })
}

async function onClearCalibration(): Promise<void> {
    const result = await calibrationStore.deleteCalibration(props.deviceUID, props.channelName)
    if (result instanceof ErrorResponse) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: result.error || t('components.channelExtensionSettings.calibration.clearError'),
            life: 6000,
        })
        return
    }
    toast.add({
        severity: 'info',
        summary: t('components.channelExtensionSettings.calibration.heading'),
        detail: t('components.channelExtensionSettings.calibration.clearedNotice'),
        life: 5000,
    })
}

// On every popover open, refresh the status once and resume polling
// if it turns out a diagnosis is mid-sweep (e.g. after page reload).
watch(isPopupOpen, (open) => {
    if (open && calibrationEligible.value) {
        calibrationStore.ensurePolling(props.deviceUID, props.channelName).catch(() => {})
    }
})

for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceUID && device.info != null) {
        const channelInfo = device.info.channels.get(props.channelName)
        if (channelInfo?.speed_options?.extension != null) {
            const channelExtensionSettings = settingsStore.ccDeviceSettings
                .get(props.deviceUID)
                ?.channel_settings.get(props.channelName)?.extension
            switch (channelInfo.speed_options.extension) {
                case ChannelExtensionNames.AmdRdnaGpu:
                    currentChannelExtension.value = ChannelExtensionNames.AmdRdnaGpu
                    if (channelExtensionSettings?.hw_fan_curve_enabled != null) {
                        hwFanCurve.value = channelExtensionSettings.hw_fan_curve_enabled
                    }
                    break
                case ChannelExtensionNames.AutoHWCurve:
                    currentChannelExtension.value = ChannelExtensionNames.AutoHWCurve
                    if (channelExtensionSettings?.auto_hw_curve_enabled != null) {
                        hwFanCurve.value = channelExtensionSettings.auto_hw_curve_enabled
                    }
                    break
                default:
                    break
            }
        }
    }
}
const hwFanCurveIsApplicable = computed((): boolean => {
    const p = props.chosenProfile
    const isApplicable =
        !!p &&
        p.p_type === ProfileType.Graph &&
        p.temp_source?.device_uid === props.deviceUID && // if it's an AMDGPU, that it uses the correct temp sensor:
        (currentChannelExtension.value !== ChannelExtensionNames.AmdRdnaGpu ||
            p.temp_source?.temp_name === 'temp1')
    if (!isApplicable) {
        nextTick(() => {
            hwFanCurve.value = false
        })
    }
    return isApplicable
})

const saveChannelExtensionSettings = async () => {
    if (currentChannelExtension.value == null || !hwFanCurveIsApplicable.value) return
    let newExtensionSettings: ChannelExtensions | undefined = undefined
    if (currentChannelExtension.value === ChannelExtensionNames.AmdRdnaGpu && hwFanCurve.value) {
        newExtensionSettings = {
            hw_fan_curve_enabled: true,
        }
    } else if (
        currentChannelExtension.value === ChannelExtensionNames.AutoHWCurve &&
        hwFanCurve.value
    ) {
        newExtensionSettings = {
            auto_hw_curve_enabled: true,
        }
    }
    const deviceSettings = settingsStore.ccDeviceSettings.get(props.deviceUID)!
    if (newExtensionSettings === undefined) {
        const ccChannelSettings = deviceSettings.channel_settings.get(props.channelName)
        if (ccChannelSettings == null) {
            // no settings exist, no need to remove the extension settings
            return
        }
        const changed = ccChannelSettings.extension != null
        ccChannelSettings.extension = undefined
        if (changed) {
            const result = await deviceStore.daemonClient.saveCCDeviceSettings(
                props.deviceUID,
                deviceSettings,
            )
            if (result instanceof ErrorResponse) {
                toast.add({
                    severity: 'error',
                    summary: t('common.error'),
                    detail: result.error || t('components.channelExtensionSettings.saveError'),
                    life: 6000,
                })
            }
        }
        return
    }
    // get or create new settings to apply
    let ccChannelSettings =
        deviceSettings.channel_settings.get(props.channelName) ?? new CCChannelSettings()
    ccChannelSettings.extension = newExtensionSettings
    deviceSettings.channel_settings.set(props.channelName, ccChannelSettings)
    const result = await deviceStore.daemonClient.saveCCDeviceSettings(
        props.deviceUID,
        deviceSettings,
    )
    if (result instanceof ErrorResponse) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: result.error || t('components.channelExtensionSettings.saveError'),
            life: 6000,
        })
    }
}
defineExpose({
    saveChannelExtensionSettings,
})
</script>

<template>
    <div
        v-tooltip.top="{
            value: t('components.channelExtensionSettings.title'),
            disabled: isPopupOpen,
        }"
    >
        <popover-root @update:open="(open) => (isPopupOpen = open)">
            <popover-trigger
                class="h-[2.375rem] rounded-lg border-2 border-border-one !py-1.5 !px-2.5 text-text-color outline-0 text-center justify-center items-center flex !m-0 hover:bg-surface-hover"
            >
                <svg-icon
                    class="outline-0 mt-[-2px]"
                    type="mdi"
                    :path="mdiCogs"
                    :size="deviceStore.getREMSize(1.35)"
                />
            </popover-trigger>
            <popover-content side="bottom" class="z-10">
                <div
                    class="w-full bg-bg-two border border-border-one p-2 rounded-lg text-text-color pb-4"
                >
                    <div class="font-semibold pb-2.5 pt-1 text-center">
                        {{ t('components.channelExtensionSettings.title') }}
                    </div>
                    <table v-if="currentChannelExtension != null">
                        <tbody>
                            <tr>
                                <td class="w-24 text-end pl-4">
                                    <div class="flex flex-row leading-none items-center">
                                        <div
                                            v-tooltip.top="
                                                t(
                                                    'components.channelExtensionSettings.firmwareControlledProfileDesc',
                                                )
                                            "
                                        >
                                            <svg-icon
                                                type="mdi"
                                                class="mr-2"
                                                :path="mdiInformationSlabCircleOutline"
                                                :size="deviceStore.getREMSize(1.25)"
                                            />
                                        </div>
                                        {{
                                            t(
                                                'components.channelExtensionSettings.firmwareControlledProfile',
                                            )
                                        }}
                                        <div
                                            class="ml-2 w-2"
                                            v-tooltip.top="
                                                t(
                                                    'components.channelExtensionSettings.firmwareControlDisabled',
                                                )
                                            "
                                        >
                                            <svg-icon
                                                v-if="!hwFanCurveIsApplicable"
                                                type="mdi"
                                                :path="mdiAlertOutline"
                                                :size="deviceStore.getREMSize(1.25)"
                                                style="color: rgb(var(--colors-red))"
                                            />
                                        </div>
                                    </div>
                                </td>
                                <td class="w-24 px-2 text-center">
                                    <el-switch
                                        v-model="hwFanCurve"
                                        size="large"
                                        :disabled="!hwFanCurveIsApplicable"
                                        @change="emit('change')"
                                    />
                                </td>
                            </tr>
                        </tbody>
                    </table>
                    <div
                        v-if="calibrationEligible"
                        :class="[
                            'px-2',
                            currentChannelExtension != null
                                ? 'mt-3 pt-3 border-t border-border-one'
                                : 'pt-1',
                        ]"
                    >
                        <!--
                          v-memo keeps PrimeVue's tooltip directive from running its
                          `updated` lifecycle on each poll-driven re-render. Without it,
                          the directive's unbindEvents() removes the visible tooltip
                          from document.body every ~1s while calibration is in_progress.
                        -->
                        <div
                            v-memo="[]"
                            v-tooltip.top="
                                t('components.channelExtensionSettings.calibration.description')
                            "
                            class="flex items-center mb-2 cursor-help"
                        >
                            <div class="leading-none">
                                <svg-icon
                                    type="mdi"
                                    class="mr-2"
                                    :path="mdiInformationSlabCircleOutline"
                                    :size="deviceStore.getREMSize(1.25)"
                                />
                            </div>
                            <span class="font-semibold">
                                {{ t('components.channelExtensionSettings.calibration.heading') }}
                            </span>
                        </div>
                        <div class="text-sm pb-2">{{ calibrationStatusText }}</div>
                        <progress-bar
                            v-if="calibrationPhase === 'in_progress'"
                            :value="
                                calibrationStatus?.phase === 'in_progress'
                                    ? calibrationStatus.percent
                                    : 0
                            "
                            :show-value="false"
                            class="mb-2"
                        />
                        <div
                            v-if="
                                calibrationPhase === 'not_started' || calibrationPhase === 'failed'
                            "
                            class="text-xs text-text-color-secondary pb-2 whitespace-pre-line"
                        >
                            {{ t('components.channelExtensionSettings.calibration.caveatsBanner') }}
                        </div>
                        <div class="flex flex-wrap gap-2">
                            <button
                                v-if="calibrationPhase === 'not_started'"
                                type="button"
                                @click="onStartCalibration"
                                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm"
                            >
                                {{
                                    t(
                                        'components.channelExtensionSettings.calibration.buttonCalibrate',
                                    )
                                }}
                            </button>
                            <button
                                v-if="calibrationPhase === 'in_progress'"
                                type="button"
                                @click="onCancelCalibration"
                                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm"
                            >
                                {{
                                    t(
                                        'components.channelExtensionSettings.calibration.buttonCancel',
                                    )
                                }}
                            </button>
                            <button
                                v-if="calibrationPhase === 'completed'"
                                type="button"
                                @click="onViewCurve"
                                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm flex items-center gap-1"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="mdiChartLine"
                                    :size="deviceStore.getREMSize(1.0)"
                                />
                                {{
                                    t(
                                        'components.channelExtensionSettings.calibration.buttonViewCurve',
                                    )
                                }}
                            </button>
                            <button
                                v-if="
                                    calibrationPhase === 'completed' ||
                                    calibrationPhase === 'failed'
                                "
                                type="button"
                                @click="onStartCalibration"
                                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm"
                            >
                                {{
                                    t(
                                        'components.channelExtensionSettings.calibration.buttonRecalibrate',
                                    )
                                }}
                            </button>
                            <button
                                v-if="
                                    calibrationPhase === 'completed' ||
                                    calibrationPhase === 'failed'
                                "
                                type="button"
                                @click="onClearCalibration"
                                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm"
                            >
                                {{
                                    t('components.channelExtensionSettings.calibration.buttonClear')
                                }}
                            </button>
                        </div>
                    </div>
                </div>
            </popover-content>
        </popover-root>
    </div>
</template>

<style scoped lang="scss">
.el-switch {
    --el-switch-on-color: rgb(var(--colors-accent));
    --el-switch-off-color: rgb(var(--colors-bg-one));
    --el-color-white: rgb(var(--colors-bg-two));
}
</style>
