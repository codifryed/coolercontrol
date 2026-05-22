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
import { computed, defineAsyncComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useToast } from 'primevue/usetoast'
import { useDialog } from 'primevue/usedialog'
import ProgressBar from 'primevue/progressbar'
import { mdiChartLine, mdiInformationSlabCircleOutline } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useCalibrationStore } from '@/stores/CalibrationStore.ts'
import { useCalibrationStatusText } from '@/composables/useCalibrationStatusText.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import type { UID } from '@/models/Device.ts'

const props = defineProps<{
    deviceUID: UID
    channelName: string
}>()

const emit = defineEmits<{
    (e: 'request-close'): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const calibrationStore = useCalibrationStore()
const toast = useToast()
const dialog = useDialog()
const { completedStatusText } = useCalibrationStatusText()

const calibrationCurveDialog = defineAsyncComponent(
    () => import('@/components/CalibrationCurveDialog.vue'),
)

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
        return completedStatusText(status.calibration)
    }
    return t('components.channelExtensionSettings.calibration.statusFailed', {
        message: status.message,
    })
})

const calibrationHasWarnings = computed((): boolean => {
    const status = calibrationStatus.value
    return status?.phase === 'completed' && (status.calibration.warnings?.length ?? 0) > 0
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
    emit('request-close')
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

// Resume polling on mount in case a sweep was already running (e.g. the
// user reloaded the page mid-calibration). Mount is the popover-open
// equivalent here: hosts mount this panel only while their popover is
// open, so the lifecycle hook fires at the right moment.
onMounted(() => {
    calibrationStore.ensurePolling(props.deviceUID, props.channelName).catch(() => {})
})
</script>

<template>
    <div>
        <!--
          v-memo keeps PrimeVue's tooltip directive from running its
          `updated` lifecycle on each poll-driven re-render. Without it,
          the directive's unbindEvents() removes the visible tooltip
          from document.body every ~1s while calibration is in_progress.
        -->
        <div class="flex items-center mb-2">
            <div
                class="leading-none cursor-help"
                v-memo="[]"
                v-tooltip.top="t('components.channelExtensionSettings.calibration.description')"
            >
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
        <div :class="['text-sm pb-2', calibrationHasWarnings ? 'text-warning' : '']">
            {{ calibrationStatusText }}
        </div>
        <progress-bar
            v-if="calibrationPhase === 'in_progress'"
            :value="calibrationStatus?.phase === 'in_progress' ? calibrationStatus.percent : 0"
            :show-value="false"
            class="mb-2"
        />
        <div
            v-if="calibrationPhase === 'not_started' || calibrationPhase === 'failed'"
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
                {{ t('components.channelExtensionSettings.calibration.buttonCalibrate') }}
            </button>
            <button
                v-if="calibrationPhase === 'in_progress'"
                type="button"
                @click="onCancelCalibration"
                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm"
            >
                {{ t('components.channelExtensionSettings.calibration.buttonCancel') }}
            </button>
            <button
                v-if="calibrationPhase === 'completed'"
                type="button"
                @click="onViewCurve"
                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm flex items-center gap-1"
            >
                <svg-icon type="mdi" :path="mdiChartLine" :size="deviceStore.getREMSize(1.0)" />
                {{ t('components.channelExtensionSettings.calibration.buttonViewCurve') }}
            </button>
            <button
                v-if="calibrationPhase === 'completed' || calibrationPhase === 'failed'"
                type="button"
                @click="onStartCalibration"
                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm"
            >
                {{ t('components.channelExtensionSettings.calibration.buttonRecalibrate') }}
            </button>
            <button
                v-if="calibrationPhase === 'completed' || calibrationPhase === 'failed'"
                type="button"
                @click="onClearCalibration"
                class="px-3 py-1 rounded border border-border-one hover:bg-surface-hover text-sm"
            >
                {{ t('components.channelExtensionSettings.calibration.buttonClear') }}
            </button>
        </div>
    </div>
</template>
