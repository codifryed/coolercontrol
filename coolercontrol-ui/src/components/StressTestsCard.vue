<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<!-- Stress tests card, extracted from the dissolved AppInfoView. PrimeVue
  widgets live here so the shell Home page stays kit-clean. -->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiAlertOutline, mdiCircle } from '@mdi/js'
import { computed, onBeforeUnmount, onMounted, type Ref, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import UiButton from '@/shell/ui/UiButton.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiSelect from '@/shell/ui/UiSelect.vue'
import { useConfirm } from '@/shell/confirm'
import { useToast } from '@/shell/toast'
import type { Device } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import HelpIcon from '@/components/info/HelpIcon.vue'
import StressTestLabel from '@/components/info/StressTestLabel.vue'
import StressBackendSelect from '@/components/info/StressBackendSelect.vue'
import {
    formatLoad,
    formatRamLoad,
    formatTemp,
    isCpu,
    isDimm,
    isGpu,
    liveLoad,
    type LiveLoad,
    trackedPeak,
} from '@/components/stressLoad.ts'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n({ useScope: 'global' })

// Stress Test
const toast = useToast()
const confirm = useConfirm()
const cpuDuration = ref<number>(60)
const gpuDuration = ref<number>(60)
const ramDuration = ref<number>(60)
const driveDuration = ref<number>(60)
const cpuActive = ref(false)
const gpuActive = ref(false)
const ramActive = ref(false)
const driveActive = ref(false)
const cpuBackend = ref<string>('built_in')
const gpuBackend = ref<string>('built_in')
const ramBackend = ref<string>('built_in')
const driveBackend = ref<string>('built_in')
const stressNgAvailable = ref(false)
const cpuLoading = ref(false)
const gpuLoading = ref(false)
const ramLoading = ref(false)
const driveLoading = ref(false)
const availableDrives = ref<Array<{ device_path: string; model?: string; size_bytes: number }>>([])
const selectedDrive = ref<string | undefined>(undefined)
const availableGpus = ref<Array<{ id: string; name: string; discrete: boolean }>>([])
// Sentinel for "stress every GPU", which is what the daemon does when no id is sent.
const ALL_GPUS = 'all'
const selectedGpu = ref<string>(ALL_GPUS)
let statusPollInterval: ReturnType<typeof setInterval> | null = null

// Bumped whenever this component sets an active flag itself. A status request already in
// flight then describes the world before that change, and applying it would undo the flag
// and tear down the interval, leaving a running test reading "Inactive" with nothing
// polling to correct it and a Start button that launches a second concurrent run.
let statusGeneration = 0
const invalidatePendingStatus = (): void => {
    statusGeneration++
}

const pollStatus = async () => {
    const generation = statusGeneration
    const status = await deviceStore.daemonClient.stressTestStatus()
    if (generation !== statusGeneration) return
    cpuActive.value = status.cpu_active
    gpuActive.value = status.gpu_active
    ramActive.value = status.ram_active
    driveActive.value = status.drive_active
    cpuBackend.value = status.cpu_backend ?? 'built_in'
    gpuBackend.value = status.gpu_backend ?? 'built_in'
    ramBackend.value = status.ram_backend ?? 'built_in'
    driveBackend.value = status.drive_backend ?? 'built_in'
    stressNgAvailable.value = status.stress_ng_available ?? false
    if (
        !status.cpu_active &&
        !status.gpu_active &&
        !status.ram_active &&
        !status.drive_active &&
        statusPollInterval
    ) {
        clearInterval(statusPollInterval)
        statusPollInterval = null
    }
}

const backendLabel = (backend: string) =>
    backend === 'stress_ng' ? 'stress-ng' : t('views.appInfo.builtInBackend')

const loadOf = (targets: (device: Device) => boolean): LiveLoad =>
    liveLoad(deviceStore.allDevices(), deviceStore.currentDeviceStatus, targets)

const cpuLive = computed(() => loadOf(isCpu))
const gpuLive = computed(() => loadOf(isGpu))
const dimmLive = computed(() => loadOf(isDimm))

// The hottest reading of each test's leading temperature, tracked while the test runs and
// left on its row afterwards. Kept here only: a reload or leaving the page forgets it.
const cpuPeak = ref<number>()
const gpuPeak = ref<number>()
const ramPeak = ref<number>()
const trackPeak = (
    peak: Ref<number | undefined>,
    active: Ref<boolean>,
    temp: () => number | undefined,
): void => {
    watch(
        [active, temp],
        ([isActive, current], previous) => {
            // The first run has no previous values.
            const wasActive = previous?.[0] ?? false
            peak.value = trackedPeak(peak.value, current, isActive, wasActive)
        },
        { immediate: true },
    )
}
trackPeak(cpuPeak, cpuActive, () => cpuLive.value.tempMax)
trackPeak(gpuPeak, gpuActive, () => gpuLive.value.tempMax)
trackPeak(ramPeak, ramActive, () => dimmLive.value.tempMax ?? cpuLive.value.tempMax)

const peakText = (peak: number | undefined): string =>
    peak === undefined ? '' : t('views.appInfo.peak', { temp: formatTemp(peak) })

// The live readout while a test runs, and its peak once it has ended. Under a steady load
// the two rarely differ, so the peak is not shown beside the live reading.
const cpuLoad = computed(() => {
    if (!cpuActive.value) return peakText(cpuPeak.value)
    return formatLoad(cpuLive.value)
})
const gpuLoad = computed(() => {
    if (!gpuActive.value) return peakText(gpuPeak.value)
    return formatLoad(gpuLive.value)
})
const ramLoad = computed(() => {
    if (!ramActive.value) return peakText(ramPeak.value)
    const labels = { dimm: t('views.appInfo.dimm'), cpu: t('models.deviceType.cpu') }
    return formatRamLoad(dimmLive.value, cpuLive.value, labels)
})

const startPolling = () => {
    invalidatePendingStatus()
    if (!statusPollInterval) {
        statusPollInterval = setInterval(pollStatus, 2000)
    }
}

const needsPsuWarning = (starting: 'cpu' | 'gpu' | 'ram' | 'drive') => {
    // Warn only when GPU and CPU/RAM would run simultaneously.
    if (starting === 'gpu') return cpuActive.value || ramActive.value
    if (starting === 'cpu' || starting === 'ram') return gpuActive.value
    return false
}

const confirmOrRun = (action: () => void, starting: 'cpu' | 'gpu' | 'ram' | 'drive') => {
    if (needsPsuWarning(starting)) {
        confirm.require({
            message: t('views.appInfo.psuWarningMessage'),
            header: t('views.appInfo.psuWarningHeader'),
            icon: mdiAlertOutline,
            defaultFocus: 'reject',
            rejectLabel: t('common.cancel'),
            acceptLabel: t('views.appInfo.proceed'),
            accept: action,
        })
    } else {
        action()
    }
}

const doCpuStress = async () => {
    cpuLoading.value = true
    const err = await deviceStore.daemonClient.startCpuStress(
        undefined,
        cpuDuration.value,
        stressNgAvailable.value ? settingsStore.cpuStressBackend : undefined,
    )
    cpuLoading.value = false
    if (err) {
        toast.add({
            severity: 'error',
            summary: t('views.appInfo.cpuStress'),
            detail: err.error,
            life: 5000,
        })
    } else {
        cpuActive.value = true
        startPolling()
    }
}
const startCpuStress = () => confirmOrRun(doCpuStress, 'cpu')

const stopCpuStress = async () => {
    cpuLoading.value = true
    await deviceStore.daemonClient.stopCpuStress()
    cpuLoading.value = false
    cpuActive.value = false
    invalidatePendingStatus()
}

// Multi-GPU rigs usually run matching cards, which report the same name.
// Those get their PCI slot appended so the two can be told apart; a lone
// card of its model stays on the plain name.
const gpuLabel = (gpu: { id: string; name: string }): string => {
    const duplicated = availableGpus.value.filter((other) => other.name === gpu.name).length > 1
    if (!duplicated) return gpu.name
    const slot = gpu.id.startsWith('0000:') ? gpu.id.slice(5) : gpu.id
    return `${gpu.name} (${slot})`
}

// Cards first, discrete ones at the top as the daemon sorted them. "All GPUs"
// trails the list: it is the fallback, not what most users are reaching for.
const gpuOptions = computed(() => [
    ...availableGpus.value.map((gpu) => ({ label: gpuLabel(gpu), value: gpu.id })),
    { label: t('views.appInfo.allGpus'), value: ALL_GPUS },
])

const doGpuStress = async () => {
    gpuLoading.value = true
    const err = await deviceStore.daemonClient.startGpuStress(
        gpuDuration.value,
        stressNgAvailable.value ? settingsStore.gpuStressBackend : undefined,
        selectedGpu.value === ALL_GPUS ? undefined : selectedGpu.value,
    )
    gpuLoading.value = false
    if (err) {
        toast.add({
            severity: 'error',
            summary: t('views.appInfo.gpuStress'),
            detail: err.error,
            life: 5000,
        })
    } else {
        gpuActive.value = true
        startPolling()
    }
}
const startGpuStress = () => confirmOrRun(doGpuStress, 'gpu')

const stopGpuStress = async () => {
    gpuLoading.value = true
    await deviceStore.daemonClient.stopGpuStress()
    gpuLoading.value = false
    gpuActive.value = false
    invalidatePendingStatus()
}

const doRamStress = async () => {
    ramLoading.value = true
    const err = await deviceStore.daemonClient.startRamStress(
        ramDuration.value,
        stressNgAvailable.value ? settingsStore.ramStressBackend : undefined,
    )
    ramLoading.value = false
    if (err) {
        toast.add({
            severity: 'error',
            summary: t('views.appInfo.ramStress'),
            detail: err.error,
            life: 5000,
        })
    } else {
        ramActive.value = true
        startPolling()
    }
}
const startRamStress = () => confirmOrRun(doRamStress, 'ram')

const stopRamStress = async () => {
    ramLoading.value = true
    await deviceStore.daemonClient.stopRamStress()
    ramLoading.value = false
    ramActive.value = false
    invalidatePendingStatus()
}

const driveOptions = computed(() =>
    availableDrives.value.map((drive) => ({
        label: driveLabel(drive),
        value: drive.device_path,
    })),
)
const driveLabel = (drive: { device_path: string; model?: string }) =>
    drive.model ? `${drive.model} (${drive.device_path})` : drive.device_path

const doDriveStress = async () => {
    if (!selectedDrive.value) return
    driveLoading.value = true
    const err = await deviceStore.daemonClient.startDriveStress(
        selectedDrive.value,
        undefined,
        driveDuration.value,
        stressNgAvailable.value ? settingsStore.driveStressBackend : undefined,
    )
    driveLoading.value = false
    if (err) {
        toast.add({
            severity: 'error',
            summary: t('views.appInfo.driveStress'),
            detail: err.error,
            life: 5000,
        })
    } else {
        driveActive.value = true
        startPolling()
    }
}
const startDriveStress = () => confirmOrRun(doDriveStress, 'drive')

const stopDriveStress = async () => {
    driveLoading.value = true
    await deviceStore.daemonClient.stopDriveStress()
    driveLoading.value = false
    driveActive.value = false
    invalidatePendingStatus()
}

const stopAllStress = async () => {
    cpuLoading.value = true
    gpuLoading.value = true
    ramLoading.value = true
    driveLoading.value = true
    await deviceStore.daemonClient.stopAllStress()
    cpuLoading.value = false
    gpuLoading.value = false
    ramLoading.value = false
    driveLoading.value = false
    cpuActive.value = false
    gpuActive.value = false
    ramActive.value = false
    driveActive.value = false
    invalidatePendingStatus()
}

onBeforeUnmount(() => {
    if (statusPollInterval) {
        clearInterval(statusPollInterval)
    }
})
onMounted(async () => {
    const drives = await deviceStore.daemonClient.listDrivesForStress()
    availableDrives.value = drives
    if (drives.length > 0) {
        selectedDrive.value = drives[0].device_path
    }
    const gpus = await deviceStore.daemonClient.listGpusForStress()
    availableGpus.value = gpus
    // The discrete card is the one with a cooler to validate. Falls back to
    // every GPU when none is discrete, which is what the daemon did before.
    selectedGpu.value = gpus.find((gpu) => gpu.discrete)?.id ?? ALL_GPUS
    await pollStatus()
    if (cpuActive.value || gpuActive.value || ramActive.value || driveActive.value) {
        startPolling()
    }
})
</script>

<template>
    <div class="xl:col-span-2 min-w-0">
        <div class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color">
            <div class="flex flex-row justify-between items-center mb-4">
                <div class="flex flex-row items-center gap-2">
                    <span class="font-semibold text-xl text-text-color">{{
                        t('views.appInfo.stressTest')
                    }}</span>
                    <HelpIcon variant="help" :text="t('views.appInfo.stressTestTooltip')" />
                </div>
                <UiButton
                    variant="danger"
                    :disabled="!cpuActive && !gpuActive && !ramActive && !driveActive"
                    @click="stopAllStress"
                    >{{ t('views.appInfo.stopAll') }}</UiButton
                >
            </div>
            <div class="overflow-x-auto">
                <table class="border-separate border-spacing-y-2 whitespace-nowrap">
                    <tbody>
                        <!-- CPU Stress -->
                        <tr>
                            <td class="pr-4">
                                <span class="font-bold text-lg">{{
                                    t('views.appInfo.cpuStress')
                                }}</span>
                            </td>
                            <td class="pr-4">
                                <UiNumberInput
                                    v-model="cpuDuration"
                                    :min="15"
                                    :max="600"
                                    :step="15"
                                    :disabled="cpuActive"
                                    suffix="s"
                                />
                            </td>
                            <td class="pr-4">
                                <UiButton
                                    v-if="!cpuActive"
                                    :disabled="ramActive"
                                    @click="startCpuStress"
                                    >{{ t('views.appInfo.start') }}</UiButton
                                >
                                <UiButton v-else variant="danger" @click="stopCpuStress">{{
                                    t('views.appInfo.stop')
                                }}</UiButton>
                            </td>
                            <td>
                                <div class="flex items-center gap-2">
                                    <span
                                        v-tooltip.top="
                                            cpuActive
                                                ? t('views.appInfo.active')
                                                : t('views.appInfo.inactive')
                                        "
                                    >
                                        <svg-icon
                                            type="mdi"
                                            :class="
                                                cpuActive
                                                    ? 'text-success'
                                                    : 'text-text-color-secondary'
                                            "
                                            :path="mdiCircle"
                                            :size="deviceStore.getREMSize(0.75)"
                                        />
                                    </span>
                                    <stress-backend-select
                                        v-if="stressNgAvailable && !cpuActive"
                                        v-model="settingsStore.cpuStressBackend"
                                        class="ml-1 stress-backend-select"
                                    />
                                    <span
                                        v-else
                                        class="text-xs text-text-color-secondary ml-1 opacity-70"
                                        >[{{ backendLabel(cpuBackend) }}]</span
                                    >
                                    <span
                                        v-if="cpuLoad"
                                        v-tooltip.top="{
                                            value: peakText(cpuPeak),
                                            disabled: !cpuActive || cpuPeak === undefined,
                                        }"
                                        class="text-sm font-medium font-numeric tabular-nums"
                                        >{{ cpuLoad }}</span
                                    >
                                </div>
                            </td>
                        </tr>
                        <!-- GPU Stress -->
                        <tr>
                            <td class="pr-4">
                                <stress-test-label
                                    :label="t('views.appInfo.gpuStress')"
                                    :tooltip="t('views.appInfo.gpuStressTooltip')"
                                />
                            </td>
                            <td class="pr-4">
                                <div class="flex items-center gap-2">
                                    <UiNumberInput
                                        v-model="gpuDuration"
                                        :min="15"
                                        :max="600"
                                        :step="15"
                                        :disabled="gpuActive"
                                        suffix="s"
                                    />
                                    <UiSelect
                                        v-model="selectedGpu"
                                        :options="gpuOptions"
                                        :placeholder="t('views.appInfo.selectGpu')"
                                        class="w-64"
                                        :disabled="gpuActive || availableGpus.length === 0"
                                    />
                                </div>
                            </td>
                            <td class="pr-4">
                                <UiButton v-if="!gpuActive" @click="startGpuStress">{{
                                    t('views.appInfo.start')
                                }}</UiButton>
                                <UiButton v-else variant="danger" @click="stopGpuStress">{{
                                    t('views.appInfo.stop')
                                }}</UiButton>
                            </td>
                            <td>
                                <div class="flex items-center gap-2">
                                    <span
                                        v-tooltip.top="
                                            gpuActive
                                                ? t('views.appInfo.active')
                                                : t('views.appInfo.inactive')
                                        "
                                    >
                                        <svg-icon
                                            type="mdi"
                                            :class="
                                                gpuActive
                                                    ? 'text-success'
                                                    : 'text-text-color-secondary'
                                            "
                                            :path="mdiCircle"
                                            :size="deviceStore.getREMSize(0.75)"
                                        />
                                    </span>
                                    <stress-backend-select
                                        v-if="stressNgAvailable && !gpuActive"
                                        v-model="settingsStore.gpuStressBackend"
                                        class="ml-1 stress-backend-select"
                                    />
                                    <span
                                        v-else
                                        class="text-xs text-text-color-secondary ml-1 opacity-70"
                                        >[{{ backendLabel(gpuBackend) }}]</span
                                    >
                                    <span
                                        v-if="gpuLoad"
                                        v-tooltip.top="{
                                            value: peakText(gpuPeak),
                                            disabled: !gpuActive || gpuPeak === undefined,
                                        }"
                                        class="text-sm font-medium font-numeric tabular-nums"
                                        >{{ gpuLoad }}</span
                                    >
                                </div>
                            </td>
                        </tr>
                        <!-- RAM Stress -->
                        <tr>
                            <td class="pr-4">
                                <span class="font-bold text-lg">{{
                                    t('views.appInfo.ramStress')
                                }}</span>
                            </td>
                            <td class="pr-4">
                                <UiNumberInput
                                    v-model="ramDuration"
                                    :min="15"
                                    :max="3600"
                                    :step="15"
                                    :disabled="ramActive"
                                    suffix="s"
                                />
                            </td>
                            <td class="pr-4">
                                <UiButton
                                    v-if="!ramActive"
                                    :disabled="cpuActive"
                                    @click="startRamStress"
                                    >{{ t('views.appInfo.start') }}</UiButton
                                >
                                <UiButton v-else variant="danger" @click="stopRamStress">{{
                                    t('views.appInfo.stop')
                                }}</UiButton>
                            </td>
                            <td>
                                <div class="flex items-center gap-2">
                                    <span
                                        v-tooltip.top="
                                            ramActive
                                                ? t('views.appInfo.active')
                                                : t('views.appInfo.inactive')
                                        "
                                    >
                                        <svg-icon
                                            type="mdi"
                                            :class="
                                                ramActive
                                                    ? 'text-success'
                                                    : 'text-text-color-secondary'
                                            "
                                            :path="mdiCircle"
                                            :size="deviceStore.getREMSize(0.75)"
                                        />
                                    </span>
                                    <stress-backend-select
                                        v-if="stressNgAvailable && !ramActive"
                                        v-model="settingsStore.ramStressBackend"
                                        class="ml-1 stress-backend-select"
                                    />
                                    <span
                                        v-else
                                        class="text-xs text-text-color-secondary ml-1 opacity-70"
                                        >[{{ backendLabel(ramBackend) }}]</span
                                    >
                                    <span
                                        v-if="ramLoad"
                                        v-tooltip.top="{
                                            value: peakText(ramPeak),
                                            disabled: !ramActive || ramPeak === undefined,
                                        }"
                                        class="text-sm font-medium font-numeric tabular-nums"
                                        >{{ ramLoad }}</span
                                    >
                                </div>
                            </td>
                        </tr>
                        <!-- Drive Stress -->
                        <tr>
                            <td class="pr-4">
                                <stress-test-label
                                    :label="t('views.appInfo.driveStress')"
                                    :tooltip="t('views.appInfo.driveStressTooltip')"
                                />
                            </td>
                            <td class="pr-4">
                                <div class="flex items-center gap-2">
                                    <UiNumberInput
                                        v-model="driveDuration"
                                        :min="15"
                                        :max="600"
                                        :step="15"
                                        :disabled="driveActive"
                                        suffix="s"
                                    />
                                    <UiSelect
                                        v-model="selectedDrive"
                                        :options="driveOptions"
                                        :placeholder="t('views.appInfo.selectDrive')"
                                        class="w-64"
                                        :disabled="driveActive || availableDrives.length === 0"
                                    />
                                </div>
                            </td>
                            <td class="pr-4">
                                <UiButton
                                    v-if="!driveActive"
                                    :disabled="!selectedDrive || availableDrives.length === 0"
                                    @click="startDriveStress"
                                    >{{ t('views.appInfo.start') }}</UiButton
                                >
                                <UiButton v-else variant="danger" @click="stopDriveStress">{{
                                    t('views.appInfo.stop')
                                }}</UiButton>
                            </td>
                            <td>
                                <div class="flex items-center gap-2">
                                    <span
                                        v-tooltip.top="
                                            driveActive
                                                ? t('views.appInfo.active')
                                                : t('views.appInfo.inactive')
                                        "
                                    >
                                        <svg-icon
                                            type="mdi"
                                            :class="
                                                driveActive
                                                    ? 'text-success'
                                                    : 'text-text-color-secondary'
                                            "
                                            :path="mdiCircle"
                                            :size="deviceStore.getREMSize(0.75)"
                                        />
                                    </span>
                                    <stress-backend-select
                                        v-if="stressNgAvailable && !driveActive"
                                        v-model="settingsStore.driveStressBackend"
                                        class="ml-1 stress-backend-select"
                                    />
                                    <span
                                        v-else
                                        class="text-xs text-text-color-secondary ml-1 opacity-70"
                                        >[{{ backendLabel(driveBackend) }}]</span
                                    >
                                </div>
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
    </div>
</template>
