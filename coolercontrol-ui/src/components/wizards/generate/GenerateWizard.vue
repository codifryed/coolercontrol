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
    mdiArrowLeft,
    mdiAutoFix,
    mdiContentSaveOutline,
    mdiInformationSlabCircleOutline,
} from '@mdi/js'
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
import { getProfileTypeDisplayName, ProfileTempSource } from '@/models/Profile.ts'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings.ts'
import {
    FanAssignment,
    FanKind,
    GenerateProfilesRequest,
    type GenerateProfilesResponse,
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

const buildRequest = (): GenerateProfilesRequest => {
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
    return request
}

// Step 4: the proposed entities, fetched but not yet persisted.
const proposal: Ref<GenerateProfilesResponse | null> = ref(null)
const building: Ref<boolean> = ref(false)
const applying: Ref<boolean> = ref(false)

const buildPreview = async (): Promise<void> => {
    building.value = true
    const response = await deviceStore.daemonClient.generateProfiles(buildRequest())
    building.value = false
    if (response == null) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('components.wizards.generate.generateError'),
            life: 4000,
        })
        return
    }
    proposal.value = response
    step.value = 4
}

const profileNameByUid = (uid: string): string =>
    proposal.value?.profiles.find((profile) => profile.uid === uid)?.name ?? uid

const fanLabel = (deviceUID: string, channelName: string): string =>
    fanRows.value.find((row) => row.deviceUID === deviceUID && row.channelName === channelName)
        ?.label ?? channelName

// The non-default profile a channel currently has, so the preview can warn before replacing it.
const currentProfileName = (deviceUID: string, channelName: string): string | undefined => {
    const profileUid = settingsStore.allDaemonDeviceSettings
        .get(deviceUID)
        ?.settings.get(channelName)?.profile_uid
    if (profileUid == null || profileUid === '0') return undefined
    return settingsStore.profiles.find((profile) => profile.uid === profileUid)?.name
}

const anyCaseFanAssigned = (): boolean =>
    fanRows.value.some((row) => row.kind === FanKind.CaseIntake || row.kind === FanKind.CaseExhaust)

const uniqueName = (base: string, existing: Set<string>): string => {
    if (!existing.has(base)) return base
    let suffix = 2
    while (existing.has(`${base} ${suffix}`)) suffix++
    return `${base} ${suffix}`
}

const applyError = (): void => {
    toast.add({
        severity: 'error',
        summary: t('common.error'),
        detail: t('components.wizards.generate.applyError'),
        life: 4000,
    })
}

// Persists the proposal in dependency order (custom sensors, functions, then profiles with
// members before Mix/Overlay parents) and applies each fan to its generated profile.
const createAndApply = async (): Promise<void> => {
    if (proposal.value == null) return
    applying.value = true
    try {
        // Custom sensor ids are the key, so rename on collision and rewire references.
        const existingSensorIds = new Set(
            (await settingsStore.getCustomSensors()).map((sensor) => sensor.id),
        )
        const sensorIdRename = new Map<string, string>()
        for (const sensor of proposal.value.custom_sensors) {
            const newId = uniqueName(sensor.id, existingSensorIds)
            existingSensorIds.add(newId)
            if (newId !== sensor.id) sensorIdRename.set(sensor.id, newId)
            sensor.id = newId
        }
        for (const profile of proposal.value.profiles) {
            const source = profile.temp_source
            if (source != null && sensorIdRename.has(source.temp_name)) {
                profile.temp_source = new ProfileTempSource(
                    sensorIdRename.get(source.temp_name)!,
                    source.device_uid,
                )
            }
        }
        for (const sensor of proposal.value.custom_sensors) {
            // Use the daemon client directly to avoid a success toast per sensor.
            if ((await deviceStore.daemonClient.saveCustomSensor(sensor)) != null) {
                applyError()
                return
            }
        }
        // Functions and profiles keep their UID, so a name suffix never breaks references.
        const existingFunctionNames = new Set(settingsStore.functions.map((fn) => fn.name))
        for (const fn of proposal.value.functions) {
            fn.name = uniqueName(fn.name, existingFunctionNames)
            existingFunctionNames.add(fn.name)
            settingsStore.functions.push(fn)
            if (!(await settingsStore.saveFunction(fn.uid))) {
                applyError()
                return
            }
        }
        const existingProfileNames = new Set(settingsStore.profiles.map((profile) => profile.name))
        for (const profile of proposal.value.profiles) {
            profile.name = uniqueName(profile.name, existingProfileNames)
            existingProfileNames.add(profile.name)
            settingsStore.profiles.push(profile)
            if (!(await settingsStore.saveProfile(profile.uid))) {
                applyError()
                return
            }
        }
        for (const assignment of proposal.value.assignments) {
            await settingsStore.saveDaemonDeviceSettingProfile(
                assignment.device_uid,
                assignment.channel_name,
                new DeviceSettingWriteProfileDTO(assignment.profile_uid),
            )
        }
    } finally {
        applying.value = false
    }
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('components.wizards.generate.generated', {
            count: proposal.value.profiles.length,
        }),
        life: 4000,
    })
    closeDialog()
    // Reload so the new profiles, functions, and custom-sensor channels surface (the menu tree is
    // event-driven and new sensors add device channels), as the custom-sensor wizard does.
    await deviceStore.waitAndReload(1)
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
        <div v-else-if="step === 3" class="flex flex-col gap-y-4">
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

        <!-- Step 4: preview -->
        <div v-else class="flex flex-col gap-y-2 overflow-y-auto">
            <small class="ml-1 font-light text-sm">
                {{ t('components.wizards.generate.previewIntro') }}
            </small>
            <div class="flex items-start gap-x-2 ml-1 mt-1">
                <svg-icon
                    type="mdi"
                    class="shrink-0 mt-0.5"
                    :path="mdiInformationSlabCircleOutline"
                    :size="deviceStore.getREMSize(1.2)"
                />
                <span class="text-sm">
                    {{ t('components.wizards.generate.startingPointNote') }}
                </span>
            </div>
            <div
                v-for="assignment in proposal?.assignments ?? []"
                :key="assignment.device_uid + assignment.channel_name"
                class="flex items-start justify-between gap-x-3"
            >
                <span class="truncate">{{
                    fanLabel(assignment.device_uid, assignment.channel_name)
                }}</span>
                <div class="text-right">
                    <span class="font-bold">{{ profileNameByUid(assignment.profile_uid) }}</span>
                    <span
                        v-if="currentProfileName(assignment.device_uid, assignment.channel_name)"
                        class="block text-xs text-yellow-500"
                    >
                        {{
                            t('components.wizards.generate.replaces', {
                                name: currentProfileName(
                                    assignment.device_uid,
                                    assignment.channel_name,
                                ),
                            })
                        }}
                    </span>
                </div>
            </div>
            <small class="ml-1 mt-2 font-light text-sm">
                {{
                    t('components.wizards.generate.willCreate', {
                        profiles: proposal?.profiles.length ?? 0,
                        functions: proposal?.functions.length ?? 0,
                        sensors: proposal?.custom_sensors.length ?? 0,
                    })
                }}
            </small>
            <div
                v-for="profile in proposal?.profiles ?? []"
                :key="profile.uid"
                class="flex items-center justify-between gap-x-3 text-sm"
            >
                <span class="truncate">{{ profile.name }}</span>
                <span class="text-text-color-secondary">{{
                    getProfileTypeDisplayName(profile.p_type)
                }}</span>
            </div>
            <small
                v-if="anyCaseFanAssigned()"
                class="ml-1 mt-1 font-light text-xs text-text-color-secondary"
            >
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
                v-else-if="step === 3"
                class="bg-accent/80 hover:!bg-accent w-32"
                :label="t('components.wizards.generate.preview')"
                :disabled="assignedCount() === 0 || building"
                @click="buildPreview"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiAutoFix"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                v-else
                class="bg-accent/80 hover:!bg-accent w-40"
                :label="t('components.wizards.generate.createApply')"
                :disabled="applying"
                @click="createAndApply"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiContentSaveOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
