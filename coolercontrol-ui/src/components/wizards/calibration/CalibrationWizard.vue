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
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiAlertCircleOutline,
    mdiCheckCircleOutline,
    mdiCircleOutline,
    mdiInformationSlabCircleOutline,
    mdiLoading,
    mdiMinusCircleOutline,
    mdiTuneVerticalVariant,
} from '@mdi/js'
import Button from 'primevue/button'
import Checkbox from 'primevue/checkbox'
import InputNumber from 'primevue/inputnumber'
import { computed, inject, onMounted, ref, type Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import { useI18n } from 'vue-i18n'
import { useToast } from 'primevue/usetoast'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useCalibrationStore } from '@/stores/CalibrationStore.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'
import type { CalibrationBatchEntry, CalibrationStage } from '@/models/Calibration.ts'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const closeDialog = (): void => dialogRef.value.close()
const { t } = useI18n()
const toast = useToast()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const calibrationStore = useCalibrationStore()

// The daemon owns the batch, so on open we resume an in-progress run
// rather than always starting at the picker.
const batchActive = (): boolean => calibrationStore.batchStatus?.active === true
const step: Ref<number> = ref(batchActive() ? 2 : 1)
onMounted(async () => {
    await calibrationStore.ensureBatchPolling()
    if (batchActive()) step.value = 2
})

// Step 1: pick the fans to calibrate (uncalibrated pre-selected).
interface FanRow {
    deviceUID: string
    channelName: string
    label: string
    color: string
    selected: boolean
    alreadyCalibrated: boolean
}
const fanRows: Ref<Array<FanRow>> = ref([])
const fillFans = (): void => {
    fanRows.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.info == null) continue
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)
        if (deviceSettings == null) continue
        for (const [channelName, channelInfo] of device.info.channels.entries()) {
            if (!(channelInfo.speed_options?.fixed_enabled ?? false)) continue
            const sc = deviceSettings.sensorsAndChannels.get(channelName)
            const calibrated =
                calibrationStore.statusFor(device.uid, channelName)?.phase === 'completed'
            fanRows.value.push({
                deviceUID: device.uid,
                channelName,
                label: sc?.name ?? channelName,
                color: sc?.color ?? '#888888',
                selected: !calibrated,
                alreadyCalibrated: calibrated,
            })
        }
    }
}
fillFans()

const selectedRows = computed(() => fanRows.value.filter((row) => row.selected))
const allSelected = computed(
    () => fanRows.value.length > 0 && fanRows.value.every((row) => row.selected),
)
const toggleAll = (): void => {
    const target = !allSelected.value
    for (const row of fanRows.value) row.selected = target
}

// How many fans to calibrate at once. 1 (sequential) is the safe default;
// the daemon clamps it to the number of selected fans.
const concurrency: Ref<number> = ref(1)
const starting: Ref<boolean> = ref(false)
const startBatch = async (): Promise<void> => {
    starting.value = true
    const channels = selectedRows.value.map((row) => ({
        device_uid: row.deviceUID,
        channel_name: row.channelName,
    }))
    const result = await calibrationStore.startBatch(channels, concurrency.value)
    starting.value = false
    if (result !== true) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail:
                result instanceof ErrorResponse
                    ? result.error
                    : t('components.wizards.calibration.startFailed'),
            life: 4000,
        })
        return
    }
    step.value = 2
}

const cancelBatch = async (): Promise<void> => {
    await calibrationStore.cancelBatch()
}

// Step 2: observe the daemon-driven batch.
const entries = computed<Array<CalibrationBatchEntry>>(
    () => calibrationStore.batchStatus?.entries ?? [],
)
const isActive = computed(() => calibrationStore.batchStatus?.active === true)
const totalCount = computed(() => entries.value.length)
const finishedCount = computed(
    () =>
        entries.value.filter((entry) => entry.phase !== 'queued' && entry.phase !== 'running')
            .length,
)
const currentNumber = computed(() => {
    const index = entries.value.findIndex((entry) => entry.phase === 'running')
    return index >= 0 ? index + 1 : finishedCount.value
})
const doneCount = computed(() => entries.value.filter((entry) => entry.phase === 'done').length)
const failedCount = computed(() => entries.value.filter((entry) => entry.phase === 'failed').length)
const skippedCount = computed(
    () => entries.value.filter((entry) => entry.phase === 'cancelled').length,
)

const entryLabel = (deviceUID: string, channelName: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)?.name ??
    channelName

const stageLabel = (stage: CalibrationStage): string => {
    switch (stage) {
        case 'preflight':
            return t('components.wizards.calibration.stagePreflight')
        case 'up_sweep':
            return t('components.wizards.calibration.stageUpSweep')
        case 'down_sweep':
            return t('components.wizards.calibration.stageDownSweep')
        case 'finalizing':
            return t('components.wizards.calibration.stageFinalizing')
    }
}
const runningText = (entry: CalibrationBatchEntry): string => {
    if (entry.stage == null || entry.percent == null)
        return t('components.wizards.calibration.queued')
    return `${stageLabel(entry.stage)} ${entry.percent}%`
}

const phaseIcon = (phase: CalibrationBatchEntry['phase']): string => {
    switch (phase) {
        case 'done':
            return mdiCheckCircleOutline
        case 'failed':
            return mdiAlertCircleOutline
        case 'cancelled':
            return mdiMinusCircleOutline
        case 'running':
            return mdiLoading
        default:
            return mdiCircleOutline
    }
}
const phaseClass = (phase: CalibrationBatchEntry['phase']): string => {
    switch (phase) {
        case 'done':
            return 'text-accent'
        case 'failed':
            return 'text-warning'
        case 'running':
            return 'text-accent animate-spin'
        default:
            return 'text-text-color-secondary'
    }
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[50vh]">
        <!-- Step 1: pick fans -->
        <div v-if="step === 1" class="flex flex-col gap-y-3 overflow-y-auto">
            <small class="ml-1 font-light text-sm">
                {{ t('components.wizards.calibration.pickIntro') }}
            </small>
            <div v-if="fanRows.length === 0" class="ml-1 text-text-color-secondary">
                {{ t('components.wizards.calibration.noFans') }}
            </div>
            <template v-else>
                <button
                    class="ml-1 self-start text-sm text-text-color-secondary hover:text-text-color"
                    @click="toggleAll"
                >
                    {{
                        allSelected
                            ? t('components.wizards.calibration.deselectAll')
                            : t('components.wizards.calibration.selectAll')
                    }}
                </button>
                <div
                    v-for="(row, index) in fanRows"
                    :key="row.deviceUID + row.channelName"
                    class="flex items-center gap-x-2 ml-1"
                >
                    <Checkbox v-model="fanRows[index].selected" binary />
                    <span class="pi pi-minus" :style="{ color: row.color }" />
                    <span class="truncate">{{ row.label }}</span>
                    <span v-if="row.alreadyCalibrated" class="text-xs text-accent">
                        {{ t('components.wizards.calibration.calibratedBadge') }}
                    </span>
                </div>
            </template>
            <template v-if="fanRows.length > 1">
                <div class="flex items-center justify-between gap-x-3 ml-1 mt-1">
                    <span class="text-sm">
                        {{ t('components.wizards.calibration.concurrencyLabel') }}
                    </span>
                    <InputNumber
                        v-model="concurrency"
                        mode="decimal"
                        show-buttons
                        button-layout="horizontal"
                        :min="1"
                        :max="fanRows.length"
                        :step="1"
                        :use-grouping="false"
                        :input-style="{ width: '3rem' }"
                    >
                        <template #incrementicon>
                            <span class="pi pi-plus" />
                        </template>
                        <template #decrementicon>
                            <span class="pi pi-minus" />
                        </template>
                    </InputNumber>
                </div>
                <span class="text-xs text-text-color-secondary ml-1">
                    {{ t('components.wizards.calibration.concurrencyNote') }}
                </span>
            </template>
            <div class="flex items-start gap-x-2 ml-1 mt-1">
                <svg-icon
                    type="mdi"
                    class="shrink-0 mt-0.5"
                    :path="mdiInformationSlabCircleOutline"
                    :size="deviceStore.getREMSize(1.2)"
                />
                <div class="flex flex-col gap-y-1">
                    <span class="text-sm">{{ t('components.wizards.calibration.idleNote') }}</span>
                    <span class="text-xs text-text-color-secondary">
                        {{ t('components.wizards.calibration.pumpCaveat') }}
                    </span>
                </div>
            </div>
        </div>

        <!-- Step 2: observe the daemon-driven batch -->
        <div v-else class="flex flex-col gap-y-2 overflow-y-auto">
            <small class="ml-1 font-light text-sm">
                {{
                    isActive
                        ? t('components.wizards.calibration.running', {
                              current: currentNumber,
                              total: totalCount,
                          })
                        : t('components.wizards.calibration.summary', {
                              done: doneCount,
                              failed: failedCount,
                              skipped: skippedCount,
                          })
                }}
            </small>
            <div
                v-for="entry in entries"
                :key="entry.device_uid + entry.channel_name"
                class="flex items-center justify-between gap-x-3 ml-1"
            >
                <div class="flex items-center gap-x-2 min-w-0">
                    <!-- overflow-hidden clips the running spinner's rotation to its own box so it
                         does not expand the scroll container and flicker a scrollbar. -->
                    <span class="shrink-0 inline-flex overflow-hidden">
                        <svg-icon
                            type="mdi"
                            :class="phaseClass(entry.phase)"
                            :path="phaseIcon(entry.phase)"
                            :size="deviceStore.getREMSize(1.2)"
                        />
                    </span>
                    <span class="truncate">{{
                        entryLabel(entry.device_uid, entry.channel_name)
                    }}</span>
                </div>
                <div class="text-right text-sm shrink-0">
                    <span v-if="entry.phase === 'running'" class="text-accent">{{
                        runningText(entry)
                    }}</span>
                    <span v-else-if="entry.phase === 'queued'" class="text-text-color-secondary">{{
                        t('components.wizards.calibration.queued')
                    }}</span>
                    <span v-else-if="entry.phase === 'done'" class="text-accent">{{
                        t('components.wizards.calibration.done')
                    }}</span>
                    <span
                        v-else-if="entry.phase === 'cancelled'"
                        class="text-text-color-secondary"
                        >{{ t('components.wizards.calibration.skipped') }}</span
                    >
                    <span v-else class="text-warning">{{
                        entry.message ?? t('components.wizards.calibration.failed')
                    }}</span>
                </div>
            </div>
        </div>

        <!-- Footer -->
        <div class="flex flex-row justify-between mt-4">
            <Button
                v-if="step === 1"
                class="w-24 bg-bg-one"
                :label="t('common.cancel')"
                @click="closeDialog"
            />
            <div v-else />
            <Button
                v-if="step === 1"
                class="bg-accent/80 hover:!bg-accent w-40"
                :label="t('components.wizards.calibration.start')"
                :disabled="selectedRows.length === 0 || starting"
                @click="startBatch"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiTuneVerticalVariant"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                v-else-if="isActive"
                class="w-28 bg-bg-one"
                :label="t('common.cancel')"
                @click="cancelBatch"
            />
            <Button
                v-else
                class="bg-accent/80 hover:!bg-accent w-28"
                :label="t('components.wizards.calibration.close')"
                @click="closeDialog"
            />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
