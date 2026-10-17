<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiAlertOutline, mdiArrowLeft, mdiAutoFix, mdiContentSaveOutline, mdiMinus } from '@mdi/js'
import PanelHeader from '@/shell/PanelHeader.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import UiScrollArea from '@/shell/ui/UiScrollArea.vue'
import UiSelect from '@/shell/ui/UiSelect.vue'
import UiGroupedSelect from '@/shell/ui/UiGroupedSelect.vue'
import UiToggleGroup from '@/shell/ui/UiToggleGroup.vue'
import { type UiOptionGroup } from '@/shell/ui/UiGroupedListbox.vue'
import { computed, inject, nextTick, ref, watch, type Ref } from 'vue'
import type { DynamicDialogInstance } from '@/shell/dialog'
import { useI18n } from 'vue-i18n'
import { useToolWizards } from '@/composables/useToolWizards.ts'
import { useToast } from '@/shell/toast'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
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
const { openCalibrationWizard } = useToolWizards()

const step: Ref<number> = ref(1)

// A single-channel preselect scopes the whole wizard to just that fan; when
// absent it enumerates every controllable fan (whole-system).
const preselect = dialogRef.value.data?.preselect as
    { deviceUID: string; channelName: string } | undefined

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
            if (
                preselect != null &&
                (device.uid !== preselect.deviceUID || channelName !== preselect.channelName)
            ) {
                continue
            }
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

// Channel names repeat across devices (Fan1, Fan2...), so the roles are assigned
// under their device. Rows are already collected device by device.
interface FanGroup {
    deviceUID: string
    deviceName: string
    rows: Array<FanRow>
}
const deviceLabel = (deviceUID: string): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.name ?? deviceUID
const fanGroups = computed<Array<FanGroup>>(() => {
    const groups: Array<FanGroup> = []
    for (const row of fanRows.value) {
        const current = groups[groups.length - 1]
        if (current != null && current.deviceUID === row.deviceUID) current.rows.push(row)
        else
            groups.push({
                deviceUID: row.deviceUID,
                deviceName: deviceLabel(row.deviceUID),
                rows: [row],
            })
    }
    return groups
})

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

// Optional hand-off to the Calibration Wizard: calibrating first makes the generated curves more
// consistent. Pre-selects the fans already given a role; if none yet, the wizard uses its default.
const calibrateFansFirst = async (): Promise<void> => {
    const preselect = fanRows.value
        .filter((row) => row.kind != null)
        .map((row) => ({ deviceUID: row.deviceUID, channelName: row.channelName }))
    closeDialog()
    await nextTick()
    openCalibrationWizard(preselect.length > 0 ? preselect : undefined)
}

// The kit select models a plain string; a cleared value (skip) maps back to null.
const setFanKind = (row: FanRow, value: string | undefined): void => {
    row.kind = (value as FanKind) ?? null
}

// Step 2: the key temps, all chosen by the user. Nothing is guessed: which temps are involved is
// a real decision (an iGPU box has no GPU temp worth following, and a simple setup may want the
// CPU temp alone), and picking a GPU temp is what opts a radiator into its GPU Mix.
interface TempOption {
    deviceUID: string
    tempName: string
    label: string
    color: string
}
interface TempGroup {
    deviceUID: string
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
        const group: TempGroup = {
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            temps: [],
        }
        for (const temp of device.status.temps) {
            const sc = deviceSettings.sensorsAndChannels.get(temp.name)
            const option: TempOption = {
                deviceUID: device.uid,
                tempName: temp.name,
                label: sc?.name ?? temp.name,
                color: sc?.color ?? '#888888',
            }
            group.temps.push(option)
        }
        if (group.temps.length > 0) tempGroups.value.push(group)
    }
}
fillTemps()

// Grouped temp selects: bridge the object model to a device/temp key string.
const tempKey = (deviceUID: string, tempName: string): string => `${deviceUID}/${tempName}`
const tempOptionGroups = computed<UiOptionGroup[]>(() =>
    tempGroups.value.map((group) => ({
        label: group.deviceName,
        color: settingsStore.allUIDeviceSettings.get(group.deviceUID)?.userColor,
        options: group.temps.map((temp) => ({
            label: temp.label,
            value: tempKey(temp.deviceUID, temp.tempName),
            color: temp.color,
        })),
    })),
)
const findTemp = (key: string | undefined): TempOption | null =>
    key == null
        ? null
        : (tempGroups.value
              .flatMap((group) => group.temps)
              .find((temp) => tempKey(temp.deviceUID, temp.tempName) === key) ?? null)
const makeTempKeyModel = (tempRef: Ref<TempOption | null>) =>
    computed<string | undefined>({
        get: () =>
            tempRef.value != null
                ? tempKey(tempRef.value.deviceUID, tempRef.value.tempName)
                : undefined,
        set: (key) => (tempRef.value = findTemp(key)),
    })
const cpuTempKey = makeTempKeyModel(cpuTemp)
const gpuTempKey = makeTempKeyModel(gpuTemp)
const liquidTempKey = makeTempKeyModel(liquidTemp)
const ambientTempKey = makeTempKeyModel(ambientTemp)

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

// Untouched override rows follow the global preset. Otherwise rows built (in goToPresets) with
// the old default stay stale and buildRequest sends them as spurious overrides, which pinned
// every kind to Balanced regardless of the chosen preset.
watch(globalPreset, (newPreset, oldPreset) => {
    for (const row of overrideRows.value) {
        if (row.preset === oldPreset) row.preset = newPreset
    }
})

// Preset toggle (single-select segmented control, mirrors the old SelectButton).
const presetToggleOptions = presetOptions.map((preset) => ({ label: preset, value: preset }))
const globalPresetModel = computed<string>({
    get: () => globalPreset.value,
    set: (value) => (globalPreset.value = value as Preset),
})
const setOverridePreset = (index: number, value: string): void => {
    overrideRows.value[index].preset = value as Preset
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
    proposal.value?.profiles.find((profile) => profile.uid === uid)?.name ??
    settingsStore.profiles.find((profile) => profile.uid === uid)?.name ??
    uid

// The daemon reuses an existing profile instead of proposing a copy, so an assignment can point at
// something already saved. Those are worth naming: on a re-run the created list is empty and this
// is all the preview has to show.
const reusedProfileNames = computed<string[]>(() => {
    const proposed = new Set((proposal.value?.profiles ?? []).map((profile) => profile.uid))
    const names = new Set<string>()
    for (const assignment of proposal.value?.assignments ?? []) {
        if (proposed.has(assignment.profile_uid)) continue
        names.add(profileNameByUid(assignment.profile_uid))
    }
    return [...names]
})
const anyEntityCreated = computed<boolean>(
    () =>
        (proposal.value?.profiles.length ?? 0) > 0 ||
        (proposal.value?.functions.length ?? 0) > 0 ||
        (proposal.value?.custom_sensors.length ?? 0) > 0,
)

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
    // saveFunction/saveProfile look their subject up in the store, so each has to be
    // pushed before it can be persisted. These hold only what is in the store but NOT yet
    // in the daemon, so a failure takes back exactly the phantom entries: anything already
    // persisted stays, or the Cooling page would omit entities the daemon really created
    // and a later drag-reorder would send the truncated list.
    const unpersistedFunctionUIDs: string[] = []
    const unpersistedProfileUIDs: string[] = []
    const abortApply = (): void => {
        for (const uid of unpersistedProfileUIDs) {
            const index = settingsStore.profiles.findIndex((profile) => profile.uid === uid)
            if (index >= 0) settingsStore.profiles.splice(index, 1)
        }
        for (const uid of unpersistedFunctionUIDs) {
            const index = settingsStore.functions.findIndex((fn) => fn.uid === uid)
            if (index >= 0) settingsStore.functions.splice(index, 1)
        }
        applyError()
        // Closed, not left open to retry: the proposal's names and custom-sensor ids
        // were already rewritten in place, so a second run would build on them.
        closeDialog()
    }
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
                abortApply()
                return
            }
        }
        // Functions and profiles keep their UID, so a name suffix never breaks references.
        const existingFunctionNames = new Set(settingsStore.functions.map((fn) => fn.name))
        for (const fn of proposal.value.functions) {
            fn.name = uniqueName(fn.name, existingFunctionNames)
            existingFunctionNames.add(fn.name)
            settingsStore.functions.push(fn)
            unpersistedFunctionUIDs.push(fn.uid)
            if (!(await settingsStore.saveFunction(fn.uid))) {
                abortApply()
                return
            }
            unpersistedFunctionUIDs.pop() // now in the daemon, so it stays in the store
        }
        const existingProfileNames = new Set(settingsStore.profiles.map((profile) => profile.name))
        for (const profile of proposal.value.profiles) {
            profile.name = uniqueName(profile.name, existingProfileNames)
            existingProfileNames.add(profile.name)
            settingsStore.profiles.push(profile)
            unpersistedProfileUIDs.push(profile.uid)
            if (!(await settingsStore.saveProfile(profile.uid))) {
                abortApply()
                return
            }
            unpersistedProfileUIDs.pop() // now in the daemon, so it stays in the store
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
    <div class="flex flex-col min-w-96 w-[40vw] h-[70vh]">
        <div class="min-h-0 flex-1">
            <UiScrollArea surface="two">
                <!-- Step 1: assign fans -->
                <div v-if="step === 1" class="flex flex-col gap-y-3">
                    <small class="ml-1 font-light text-sm">
                        {{ t('components.wizards.generate.assignIntro') }}
                    </small>
                    <button
                        type="button"
                        class="ml-1 self-start text-sm text-text-color-secondary hover:underline"
                        @click="calibrateFansFirst"
                    >
                        {{ t('components.wizards.generate.calibrateFirst') }}
                    </button>
                    <div v-if="fanRows.length === 0" class="ml-1 text-text-color-secondary">
                        {{ t('components.wizards.generate.noFans') }}
                    </div>
                    <template v-for="group in fanGroups" :key="group.deviceUID">
                        <PanelHeader :label="group.deviceName" />
                        <div
                            v-for="row in group.rows"
                            :key="row.deviceUID + row.channelName"
                            class="flex items-center justify-between gap-x-3"
                        >
                            <div class="flex items-center min-w-0">
                                <svg-icon
                                    type="mdi"
                                    :path="mdiMinus"
                                    :size="16"
                                    class="mr-2 ml-1"
                                    :style="{ color: row.color }"
                                />
                                <span class="truncate">{{ row.label }}</span>
                            </div>
                            <UiSelect
                                :model-value="row.kind ?? undefined"
                                :options="kindOptions"
                                clearable
                                :placeholder="t('components.wizards.generate.skip')"
                                class="w-56"
                                @update:model-value="setFanKind(row, $event)"
                            />
                        </div>
                    </template>
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
                            {
                                label: t('components.wizards.generate.ambientTemp'),
                                model: 'ambient',
                            },
                        ]"
                        :key="picker.model"
                        class="flex items-center justify-between gap-x-3"
                    >
                        <span class="ml-1">{{ picker.label }}</span>
                        <UiGroupedSelect
                            v-if="picker.model === 'cpu'"
                            v-model="cpuTempKey"
                            :groups="tempOptionGroups"
                            clearable
                            filter
                            :filter-placeholder="t('common.search')"
                            :placeholder="t('components.wizards.generate.tempNone')"
                            class="w-64"
                        />
                        <UiGroupedSelect
                            v-else-if="picker.model === 'gpu'"
                            v-model="gpuTempKey"
                            :groups="tempOptionGroups"
                            clearable
                            filter
                            :filter-placeholder="t('common.search')"
                            :placeholder="t('components.wizards.generate.tempNone')"
                            class="w-64"
                        />
                        <UiGroupedSelect
                            v-else-if="picker.model === 'liquid'"
                            v-model="liquidTempKey"
                            :groups="tempOptionGroups"
                            clearable
                            filter
                            :filter-placeholder="t('common.search')"
                            :placeholder="t('components.wizards.generate.tempNone')"
                            class="w-64"
                        />
                        <UiGroupedSelect
                            v-else
                            v-model="ambientTempKey"
                            :groups="tempOptionGroups"
                            clearable
                            filter
                            :filter-placeholder="t('common.search')"
                            :placeholder="t('components.wizards.generate.tempNone')"
                            class="w-64"
                        />
                    </div>
                </div>

                <!-- Step 3: preset -->
                <div v-else-if="step === 3" class="flex flex-col gap-y-4">
                    <small class="ml-1 font-light text-sm">
                        {{ t('components.wizards.generate.presetIntro') }}
                    </small>
                    <UiToggleGroup
                        v-model="globalPresetModel"
                        :options="presetToggleOptions"
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
                            <UiToggleGroup
                                :model-value="overrideRows[index].preset"
                                :options="presetToggleOptions"
                                @update:model-value="setOverridePreset(index, $event)"
                            />
                        </div>
                    </div>
                    <small class="ml-1 font-light text-xs text-text-color-secondary">
                        {{ t('components.wizards.generate.cfmCaveat') }}
                    </small>
                </div>

                <!-- Step 4: preview -->
                <div v-else class="flex flex-col gap-y-3">
                    <small class="ml-1 font-light text-sm">
                        {{ t('components.wizards.generate.previewIntro') }}
                    </small>

                    <!-- Fan assignments -->
                    <div class="flex flex-col gap-y-1">
                        <div class="ml-1 pb-1 border-b border-border-one text-sm font-semibold">
                            {{ t('components.wizards.generate.previewAssignments') }}
                        </div>
                        <div
                            v-for="assignment in proposal?.assignments ?? []"
                            :key="assignment.device_uid + assignment.channel_name"
                            class="flex items-start justify-between gap-x-3 ml-1"
                        >
                            <div class="flex min-w-0 items-baseline gap-x-2">
                                <span class="truncate">{{
                                    fanLabel(assignment.device_uid, assignment.channel_name)
                                }}</span>
                                <span class="truncate text-sm text-text-color-secondary">{{
                                    deviceLabel(assignment.device_uid)
                                }}</span>
                            </div>
                            <div class="text-right">
                                <span class="font-bold">{{
                                    profileNameByUid(assignment.profile_uid)
                                }}</span>
                                <span
                                    v-if="
                                        currentProfileName(
                                            assignment.device_uid,
                                            assignment.channel_name,
                                        )
                                    "
                                    class="block text-xs text-warning"
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
                    </div>

                    <!-- Reused: already exists, so nothing is created for it -->
                    <div v-if="reusedProfileNames.length > 0" class="flex flex-col gap-y-1">
                        <div class="ml-1 pb-1 border-b border-border-one text-sm font-semibold">
                            {{ t('components.wizards.generate.reusedHeader') }}
                        </div>
                        <div
                            v-for="name in reusedProfileNames"
                            :key="name"
                            class="flex items-center justify-between gap-x-3 ml-1 text-sm"
                        >
                            <span class="truncate">{{ name }}</span>
                            <span class="shrink-0 text-text-color-secondary">
                                {{ t('components.wizards.generate.reused') }}
                            </span>
                        </div>
                    </div>

                    <!-- Will be created -->
                    <div v-if="anyEntityCreated" class="flex flex-col gap-y-1">
                        <div class="ml-1 pb-1 border-b border-border-one text-sm font-semibold">
                            {{ t('components.wizards.generate.willCreateHeader') }}
                        </div>
                        <!-- A warning, not an aside: a generated setup nearly always needs
                             tweaking, and the user should know that before creating it. -->
                        <div class="flex items-start gap-x-2 ml-1 mb-1">
                            <svg-icon
                                type="mdi"
                                class="shrink-0 mt-0.5 text-warning"
                                :path="mdiAlertOutline"
                                :size="deviceStore.getREMSize(1.2)"
                            />
                            <span class="text-sm">
                                {{ t('components.wizards.generate.startingPointNote') }}
                            </span>
                        </div>
                        <div
                            v-for="profile in proposal?.profiles ?? []"
                            :key="profile.uid"
                            class="flex items-center justify-between gap-x-3 ml-1 text-sm"
                        >
                            <span class="truncate">{{ profile.name }}</span>
                            <span class="shrink-0 text-text-color-secondary">{{
                                `${getProfileTypeDisplayName(profile.p_type)} ${t('layout.add.profile')}`
                            }}</span>
                        </div>
                        <div
                            v-for="fn in proposal?.functions ?? []"
                            :key="fn.uid"
                            class="flex items-center justify-between gap-x-3 ml-1 text-sm"
                        >
                            <span class="truncate">{{ fn.name }}</span>
                            <span class="shrink-0 text-text-color-secondary">{{
                                t('layout.add.function')
                            }}</span>
                        </div>
                        <div
                            v-for="sensor in proposal?.custom_sensors ?? []"
                            :key="sensor.id"
                            class="flex items-center justify-between gap-x-3 ml-1 text-sm"
                        >
                            <span class="truncate">{{ sensor.id }}</span>
                            <span class="shrink-0 text-text-color-secondary">{{
                                t('layout.add.customSensor')
                            }}</span>
                        </div>
                        <small
                            v-if="anyCaseFanAssigned()"
                            class="ml-1 mt-1 font-light text-xs text-text-color-secondary"
                        >
                            {{ t('components.wizards.generate.cfmCaveat') }}
                        </small>
                    </div>
                </div>
            </UiScrollArea>
        </div>

        <!-- Footer -->
        <div class="flex flex-row justify-between mt-4 shrink-0">
            <UiButton v-if="step === 1" variant="ghost" class="w-24 bg-bg-one" @click="closeDialog">
                {{ t('common.cancel') }}
            </UiButton>
            <UiButton v-else variant="ghost" class="w-24 bg-bg-one" @click="step = step - 1">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </UiButton>
            <UiButton
                v-if="step === 1"
                variant="ghost"
                class="w-24 bg-bg-one"
                :disabled="assignedCount() === 0"
                @click="step = 2"
            >
                {{ t('common.next') }}
            </UiButton>
            <UiButton
                v-else-if="step === 2"
                variant="ghost"
                class="w-24 bg-bg-one"
                @click="goToPresets"
            >
                {{ t('common.next') }}
            </UiButton>
            <UiButton
                v-else-if="step === 3"
                variant="solid"
                class="w-32"
                :disabled="assignedCount() === 0 || building"
                @click="buildPreview"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiAutoFix"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </UiButton>
            <UiButton
                v-else
                variant="solid"
                class="w-40"
                :disabled="applying"
                @click="createAndApply"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiContentSaveOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </UiButton>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
