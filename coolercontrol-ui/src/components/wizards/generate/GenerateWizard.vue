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
import { mdiArrowLeft, mdiAutoFix } from '@mdi/js'
import Button from 'primevue/button'
import Select from 'primevue/select'
import SelectButton from 'primevue/selectbutton'
import { inject, ref, type Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import { useI18n } from 'vue-i18n'
import { useToast } from 'primevue/usetoast'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { DeviceType } from '@/models/Device.ts'
import { ProfileTempSource } from '@/models/Profile.ts'
import {
    FanAssignment,
    FanKind,
    GenerateProfilesRequest,
    KeyTemps,
    Preset,
    PresetOverride,
} from '@/models/ProfileGeneration.ts'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const closeDialog = (): void => dialogRef.value.close()
const { t } = useI18n()
const toast = useToast()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const step: Ref<number> = ref(1)

// Step 1: assign each controllable fan a role (or leave unset to skip).
interface FanRow {
    deviceUID: string
    channelName: string
    label: string
    color: string
    kind: FanKind | null
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
            fanRows.value.push({
                deviceUID: device.uid,
                channelName,
                label: sc?.name ?? channelName,
                color: sc?.color ?? '#888888',
                kind: null,
            })
        }
    }
}
fillFans()

const kindOptions = [
    FanKind.CpuCooler,
    FanKind.GpuFan,
    FanKind.AioRadiator,
    FanKind.AioPump,
    FanKind.CaseIntake,
    FanKind.CaseExhaust,
    FanKind.LaptopFan,
].map((kind) => ({ value: kind, label: t(`components.wizards.generate.kind.${kind}`) }))

const assignedCount = (): number => fanRows.value.filter((row) => row.kind != null).length

// The shared Select preset positions the clear (X) icon at right-12, which leaves it stranded
// mid-field here because this preset puts the dropdown chevron on the left. Move the X flush to
// the right edge and reserve just enough label padding so the text does not run under it.
const selectPt = {
    label: { class: '!pr-9' },
    clearIcon: { class: '!right-3' },
}

// Step 2: confirm the key temps. Pre-filled by a best-guess heuristic the user must verify.
interface TempOption {
    deviceUID: string
    tempName: string
    label: string
    color: string
}
interface TempGroup {
    deviceName: string
    temps: Array<TempOption>
}
const tempGroups: Ref<Array<TempGroup>> = ref([])
const cpuTemp: Ref<TempOption | null> = ref(null)
const gpuTemp: Ref<TempOption | null> = ref(null)
const liquidTemp: Ref<TempOption | null> = ref(null)
const ambientTemp: Ref<TempOption | null> = ref(null)

const fillTemps = (): void => {
    tempGroups.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.status.temps.length === 0 || device.info == null) continue
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)
        if (deviceSettings == null) continue
        const group: TempGroup = { deviceName: deviceSettings.name, temps: [] }
        for (const temp of device.status.temps) {
            const sc = deviceSettings.sensorsAndChannels.get(temp.name)
            const option: TempOption = {
                deviceUID: device.uid,
                tempName: temp.name,
                label: sc?.name ?? temp.name,
                color: sc?.color ?? '#888888',
            }
            group.temps.push(option)
            if (cpuTemp.value == null && device.type === DeviceType.CPU) cpuTemp.value = option
            if (gpuTemp.value == null && device.type === DeviceType.GPU) gpuTemp.value = option
            if (liquidTemp.value == null && temp.name.toLowerCase().includes('liquid'))
                liquidTemp.value = option
        }
        if (group.temps.length > 0) tempGroups.value.push(group)
    }
}
fillTemps()

// Step 3: choose the global preset, with optional per-role overrides.
const presetOptions = [Preset.Silent, Preset.Balanced, Preset.Performance]
const globalPreset: Ref<Preset> = ref(Preset.Balanced)
const overridesOpen: Ref<boolean> = ref(false)

interface OverrideRow {
    key: string
    label: string
    kinds: Array<FanKind>
    preset: Preset
}
const overrideRows: Ref<Array<OverrideRow>> = ref([])
const buildOverrideRows = (): void => {
    const assigned = new Set(
        fanRows.value.map((row) => row.kind).filter((kind): kind is FanKind => kind != null),
    )
    const rows: Array<OverrideRow> = []
    for (const kind of kindOptions.map((option) => option.value)) {
        if (kind === FanKind.CaseIntake || kind === FanKind.CaseExhaust) continue
        if (assigned.has(kind)) {
            rows.push({
                key: kind,
                label: t(`components.wizards.generate.kind.${kind}`),
                kinds: [kind],
                preset: globalPreset.value,
            })
        }
    }
    // Case intake and exhaust are coupled into one row (they share a base and preset).
    if (assigned.has(FanKind.CaseIntake) || assigned.has(FanKind.CaseExhaust)) {
        rows.push({
            key: 'case',
            label: 'Case fans',
            kinds: [FanKind.CaseIntake, FanKind.CaseExhaust],
            preset: globalPreset.value,
        })
    }
    overrideRows.value = rows
}

const goToPresets = (): void => {
    buildOverrideRows()
    step.value = 3
}

const toTempSource = (option: TempOption | null): ProfileTempSource | undefined =>
    option == null ? undefined : new ProfileTempSource(option.tempName, option.deviceUID)

const generating: Ref<boolean> = ref(false)
const generate = async (): Promise<void> => {
    const request = new GenerateProfilesRequest()
    request.global_preset = globalPreset.value
    request.assignments = fanRows.value
        .filter((row) => row.kind != null)
        .map((row) => new FanAssignment(row.deviceUID, row.channelName, row.kind as FanKind))
    const keyTemps = new KeyTemps()
    keyTemps.cpu = toTempSource(cpuTemp.value)
    keyTemps.gpu = toTempSource(gpuTemp.value)
    keyTemps.liquid = toTempSource(liquidTemp.value)
    keyTemps.ambient = toTempSource(ambientTemp.value)
    request.key_temps = keyTemps
    const overrides: Array<PresetOverride> = []
    for (const row of overrideRows.value) {
        if (row.preset !== globalPreset.value) {
            for (const kind of row.kinds) overrides.push(new PresetOverride(kind, row.preset))
        }
    }
    request.preset_overrides = overrides

    generating.value = true
    const response = await deviceStore.daemonClient.generateProfiles(request)
    generating.value = false
    if (response == null) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('components.wizards.generate.generateError'),
            life: 4000,
        })
        return
    }
    // Phase 4 will render a preview and a Create & Apply step. For now, report the result.
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('components.wizards.generate.generated', { count: response.profiles.length }),
        life: 4000,
    })
    closeDialog()
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[50vh]">
        <!-- Step 1: assign fans -->
        <div v-if="step === 1" class="flex flex-col gap-y-3 overflow-y-auto">
            <small class="ml-1 font-light text-sm">
                {{ t('components.wizards.generate.assignIntro') }}
            </small>
            <div v-if="fanRows.length === 0" class="ml-1 text-text-color-secondary">
                {{ t('components.wizards.generate.noFans') }}
            </div>
            <div
                v-for="(row, index) in fanRows"
                :key="row.deviceUID + row.channelName"
                class="flex items-center justify-between gap-x-3"
            >
                <div class="flex items-center min-w-0">
                    <span class="pi pi-minus mr-2 ml-1" :style="{ color: row.color }" />
                    <span class="truncate">{{ row.label }}</span>
                </div>
                <Select
                    v-model="fanRows[index].kind"
                    :options="kindOptions"
                    option-label="label"
                    option-value="value"
                    class="w-56 h-10"
                    show-clear
                    :pt="selectPt"
                    :pt-options="{ mergeProps: true }"
                    :placeholder="t('components.wizards.generate.skip')"
                />
            </div>
        </div>

        <!-- Step 2: key temps -->
        <div v-else-if="step === 2" class="flex flex-col gap-y-3">
            <small class="ml-1 font-light text-sm">
                {{ t('components.wizards.generate.tempsIntro') }}
            </small>
            <div
                v-for="picker in [
                    { label: t('components.wizards.generate.cpuTemp'), model: 'cpu' },
                    { label: t('components.wizards.generate.gpuTemp'), model: 'gpu' },
                    { label: t('components.wizards.generate.liquidTemp'), model: 'liquid' },
                    { label: t('components.wizards.generate.ambientTemp'), model: 'ambient' },
                ]"
                :key="picker.model"
                class="flex items-center justify-between gap-x-3"
            >
                <span class="ml-1">{{ picker.label }}</span>
                <Select
                    v-if="picker.model === 'cpu'"
                    v-model="cpuTemp"
                    :pt="selectPt"
                    :pt-options="{ mergeProps: true }"
                    :options="tempGroups"
                    option-label="label"
                    option-group-label="deviceName"
                    option-group-children="temps"
                    class="w-64 h-10"
                    show-clear
                    filter
                    :filter-placeholder="t('common.search')"
                    :placeholder="t('components.wizards.generate.tempNone')"
                />
                <Select
                    v-else-if="picker.model === 'gpu'"
                    v-model="gpuTemp"
                    :pt="selectPt"
                    :pt-options="{ mergeProps: true }"
                    :options="tempGroups"
                    option-label="label"
                    option-group-label="deviceName"
                    option-group-children="temps"
                    class="w-64 h-10"
                    show-clear
                    filter
                    :filter-placeholder="t('common.search')"
                    :placeholder="t('components.wizards.generate.tempNone')"
                />
                <Select
                    v-else-if="picker.model === 'liquid'"
                    v-model="liquidTemp"
                    :pt="selectPt"
                    :pt-options="{ mergeProps: true }"
                    :options="tempGroups"
                    option-label="label"
                    option-group-label="deviceName"
                    option-group-children="temps"
                    class="w-64 h-10"
                    show-clear
                    filter
                    :filter-placeholder="t('common.search')"
                    :placeholder="t('components.wizards.generate.tempNone')"
                />
                <Select
                    v-else
                    v-model="ambientTemp"
                    :pt="selectPt"
                    :pt-options="{ mergeProps: true }"
                    :options="tempGroups"
                    option-label="label"
                    option-group-label="deviceName"
                    option-group-children="temps"
                    class="w-64 h-10"
                    show-clear
                    filter
                    :filter-placeholder="t('common.search')"
                    :placeholder="t('components.wizards.generate.tempNone')"
                />
            </div>
        </div>

        <!-- Step 3: preset -->
        <div v-else class="flex flex-col gap-y-4">
            <small class="ml-1 font-light text-sm">
                {{ t('components.wizards.generate.presetIntro') }}
            </small>
            <SelectButton
                v-model="globalPreset"
                :options="presetOptions"
                :allow-empty="false"
                class="self-start"
            />
            <button
                class="ml-1 text-sm text-text-color-secondary hover:text-text-color self-start"
                @click="overridesOpen = !overridesOpen"
            >
                {{ t('components.wizards.generate.perKindOverrides') }}
            </button>
            <div v-if="overridesOpen" class="flex flex-col gap-y-2">
                <div
                    v-for="(row, index) in overrideRows"
                    :key="row.key"
                    class="flex items-center justify-between gap-x-3"
                >
                    <span class="ml-1">{{ row.label }}</span>
                    <SelectButton
                        v-model="overrideRows[index].preset"
                        :options="presetOptions"
                        :allow-empty="false"
                    />
                </div>
            </div>
            <small class="ml-1 font-light text-xs text-text-color-secondary">
                {{ t('components.wizards.generate.cfmCaveat') }}
            </small>
        </div>

        <!-- Footer -->
        <div class="flex flex-row justify-between mt-4">
            <Button
                v-if="step === 1"
                class="w-24 bg-bg-one"
                :label="t('common.cancel')"
                @click="closeDialog"
            />
            <Button
                v-else
                class="w-24 bg-bg-one"
                :label="t('common.back')"
                @click="step = step - 1"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                v-if="step === 1"
                class="w-24 bg-bg-one"
                :label="t('common.next')"
                :disabled="assignedCount() === 0"
                @click="step = 2"
            />
            <Button
                v-else-if="step === 2"
                class="w-24 bg-bg-one"
                :label="t('common.next')"
                @click="goToPresets"
            />
            <Button
                v-else
                class="bg-accent/80 hover:!bg-accent w-32"
                :label="t('components.wizards.generate.generate')"
                :disabled="assignedCount() === 0 || generating"
                @click="generate"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiAutoFix"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
