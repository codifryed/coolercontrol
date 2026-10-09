<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiAlertOutline,
    mdiBookmarkCheck,
    mdiBookmarkMinusOutline,
    mdiBookmarkMultipleOutline,
} from '@mdi/js'
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useToolWizards } from '@/composables/useToolWizards.ts'
import { useToast } from '@/shell/toast'
import UiButton from '@/shell/ui/UiButton.vue'
import UiSelect, { type UiSelectOption } from '@/shell/ui/UiSelect.vue'
import UiSettingRow from '@/shell/ui/UiSettingRow.vue'
import UiSettingsCard from '@/shell/ui/UiSettingsCard.vue'
import {
    hasTranslatedLabel,
    unappliedProfileMode,
    UNAPPLIED_MODE_GRACE_MS,
    useSustained,
} from '@/shell/cooling/powerProfiles.ts'

const { t } = useI18n()
const settingsStore = useSettingsStore()
const toast = useToast()
const { openModeWizard } = useToolWizards()

const activate = async (modeUID: string): Promise<void> => {
    await settingsStore.activateMode(modeUID)
}

// Only the active profile is pushed, so each visit fetches the profiles the system offers now.
onMounted(() => settingsStore.loadPowerProfiles())

// Only shown when the daemon actually reached a power profile daemon over D-Bus. On a system
// without one (TLP only, or none at all) there is nothing to map, so the card stays hidden.
const showPowerProfiles = computed(() => settingsStore.powerProfilesAvailable.length > 0)

// The "no mode" row clears the mapping for that profile. It carries a sentinel
// rather than an empty string: an empty value is reserved for clearing a Select.
const NO_MODE = 'none'
const modeOptions = computed<UiSelectOption[]>(() => [
    { label: t('layout.shell.coolingPage.powerProfiles.noMode'), value: NO_MODE },
    ...settingsStore.modes.map((mode) => ({ label: mode.name, value: mode.uid })),
])

const profileLabel = (profile: string): string =>
    hasTranslatedLabel(profile)
        ? t(`layout.shell.coolingPage.powerProfiles.profileNames.${profile}`)
        : profile

// Undefined when there is nothing to warn about, and while a mismatch is still fresh: every
// profile switch opens one until its Mode is active.
const unappliedModeName = useSustained<string>(
    () => {
        const modeUID = unappliedProfileMode(
            settingsStore.powerProfileActive,
            settingsStore.powerProfileModes,
            settingsStore.modeActiveCurrent,
        )
        return settingsStore.modes.find((mode) => mode.uid === modeUID)?.name
    },
    UNAPPLIED_MODE_GRACE_MS,
    () => settingsStore.powerProfileActive,
)

const mappedMode = (profile: string): string => {
    const modeUID = settingsStore.powerProfileModes[profile]
    return modeUID != null && modeUID !== '' ? modeUID : NO_MODE
}

const setMappedMode = async (profile: string, modeUID: string | undefined): Promise<void> => {
    const saved = await settingsStore.savePowerProfileModes({
        ...settingsStore.powerProfileModes,
        [profile]: modeUID == null || modeUID === NO_MODE ? '' : modeUID,
    })
    if (saved) return
    toast.add({
        severity: 'error',
        summary: t('common.error'),
        detail: t('layout.shell.coolingPage.powerProfiles.saveFailed'),
        life: 3000,
    })
    // Put the picker back on whatever the daemon actually holds.
    await settingsStore.loadPowerProfiles()
}
</script>

<template>
    <div class="flex h-full flex-col overflow-y-auto p-4">
        <div class="flex items-center justify-between">
            <h1 class="text-xl font-semibold text-text-color">{{ t('layout.shell.modes') }}</h1>
            <UiButton variant="outline" @click="openModeWizard()">
                {{ t('views.modes.createMode') }}
            </UiButton>
        </div>
        <div v-if="settingsStore.modes.length > 0" class="mt-4 flex flex-col gap-2">
            <!-- The whole row opens the mode, as the cooling cards do. Activate
                 sits inside it, so it stops the click from following the link.
                 The active row carries no Activate button, and the height it
                 gives the others (min-h-14: py-2 either side of a 2.5rem
                 button) is held so the list does not step. -->
            <RouterLink
                v-for="mode in settingsStore.modes"
                :key="mode.uid"
                :to="{ name: 'modes', params: { modeUID: mode.uid } }"
                class="flex min-h-14 items-center gap-3 rounded-lg border border-border-one bg-bg-two px-3 py-2 outline-none transition-colors hover:bg-surface-hover focus-visible:ring-2 focus-visible:ring-accent"
            >
                <svg-icon
                    type="mdi"
                    :path="
                        mode.uid === settingsStore.modeActiveCurrent
                            ? mdiBookmarkCheck
                            : mode.uid === settingsStore.modeActivePrevious
                              ? mdiBookmarkMinusOutline
                              : mdiBookmarkMultipleOutline
                    "
                    :size="18"
                    :class="
                        mode.uid === settingsStore.modeActiveCurrent
                            ? 'text-accent'
                            : 'text-text-color-secondary'
                    "
                />
                <span class="truncate text-text-color">{{ mode.name }}</span>
                <span
                    v-if="mode.uid === settingsStore.modeActiveCurrent"
                    class="text-sm text-accent"
                >
                    {{ t('layout.shell.coolingPage.activeMode') }}
                </span>
                <span
                    v-else-if="mode.uid === settingsStore.modeActivePrevious"
                    class="text-sm text-text-color-secondary"
                >
                    {{ t('layout.shell.coolingPage.previousMode') }}
                </span>
                <UiButton
                    v-if="mode.uid !== settingsStore.modeActiveCurrent"
                    variant="outline"
                    class="ml-auto"
                    @click.stop.prevent="activate(mode.uid)"
                >
                    {{ t('layout.shell.coolingPage.activate') }}
                </UiButton>
            </RouterLink>
        </div>
        <div v-else class="flex flex-1 items-center justify-center text-text-color-secondary">
            {{ t('layout.shell.coolingPage.noModes') }}
        </div>
        <UiSettingsCard
            v-if="showPowerProfiles"
            class="mt-6"
            :title="t('layout.shell.coolingPage.powerProfiles.title')"
        >
            <UiSettingRow
                :label="t('layout.shell.coolingPage.powerProfiles.description')"
                :description="
                    settingsStore.powerProfileActive
                        ? t('layout.shell.coolingPage.powerProfiles.activeProfile', {
                              profile: profileLabel(settingsStore.powerProfileActive),
                          })
                        : ''
                "
            />
            <div
                v-if="unappliedModeName != null"
                class="flex items-start gap-2 px-4 py-3 text-base text-text-color"
                role="status"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiAlertOutline"
                    :size="18"
                    class="mt-0.5 shrink-0 text-warning"
                />
                <!-- Two blocks, so no separator is imposed on languages that use none. -->
                <div>
                    <p>
                        {{
                            t('layout.shell.coolingPage.powerProfiles.modeNotActive', {
                                mode: unappliedModeName,
                            })
                        }}
                    </p>
                    <!-- With apply-on-boot off the daemon activates nothing at startup. -->
                    <p v-if="settingsStore.ccSettings.apply_on_boot">
                        {{ t('layout.shell.coolingPage.powerProfiles.modeNotActiveRestart') }}
                    </p>
                </div>
            </div>
            <UiSettingRow
                v-for="profile in settingsStore.powerProfilesAvailable"
                :key="profile"
                :label="profileLabel(profile)"
            >
                <UiSelect
                    class="w-56"
                    :options="modeOptions"
                    :model-value="mappedMode(profile)"
                    @update:model-value="setMappedMode(profile, $event)"
                />
            </UiSettingRow>
        </UiSettingsCard>
    </div>
</template>
