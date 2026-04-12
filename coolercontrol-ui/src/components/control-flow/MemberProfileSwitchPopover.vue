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
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import Popover from 'primevue/popover'
import Button from 'primevue/button'
import { useSettingsStore } from '@/stores/SettingsStore'
import { ProfileType } from '@/models/Profile'
import type { UID } from '@/models/Device'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiCheck } from '@mdi/js'

const props = defineProps<{
    profileUID: UID
    profileType: ProfileType
    currentMemberUIDs?: UID[]
}>()

const emit = defineEmits<{
    (e: 'changed'): void
}>()

const { t } = useI18n()
const settingsStore = useSettingsStore()
const popRef = ref()

const isOverlay = computed(() => props.profileType === ProfileType.Overlay)

const typeBadgeClass: Record<string, string> = {
    [ProfileType.Default]: 'bg-info/20 text-info',
    [ProfileType.Fixed]: 'bg-success/20 text-success',
    [ProfileType.Graph]: 'bg-accent/20 text-accent',
    [ProfileType.Mix]: 'bg-pink/20 text-pink',
    [ProfileType.Overlay]: 'bg-warning/20 text-warning',
}

// Eligible profiles: Graph + non-nested Mix, excluding self
const eligibleProfiles = computed(() =>
    settingsStore.profiles.filter((profile) => {
        if (profile.uid === props.profileUID) return false
        if (profile.uid === '0') return false
        if (profile.p_type === ProfileType.Graph) return true
        if (profile.p_type === ProfileType.Fixed) return true
        if (profile.p_type !== ProfileType.Mix) return false
        // Exclude Mix profiles that already have Mix sub-members
        const hasMixSubMembers = profile.member_profile_uids.some(
            (uid) => settingsStore.profiles.find((p) => p.uid === uid)?.p_type === ProfileType.Mix,
        )
        return !hasMixSubMembers
    }),
)

// For Mix mode: track selected members locally
const selectedUIDs = ref<Set<UID>>(new Set(props.currentMemberUIDs ?? []))

watch(
    () => props.currentMemberUIDs,
    (uids) => {
        selectedUIDs.value = new Set(uids ?? [])
    },
)

function toggle(event: Event) {
    selectedUIDs.value = new Set(props.currentMemberUIDs ?? [])
    popRef.value?.toggle(event)
}

function toggleMember(uid: UID) {
    if (selectedUIDs.value.has(uid)) {
        selectedUIDs.value.delete(uid)
    } else {
        selectedUIDs.value.add(uid)
    }
}

// Overlay: single-select, immediate save
async function selectBase(uid: UID) {
    if (props.currentMemberUIDs?.[0] === uid) {
        popRef.value?.hide()
        return
    }
    const profile = settingsStore.profiles.find((p) => p.uid === props.profileUID)
    if (!profile) return
    profile.member_profile_uids = [uid]
    await settingsStore.updateProfile(props.profileUID)
    popRef.value?.hide()
    emit('changed')
}

// Mix: multi-select, save on button click
async function applyMembers() {
    const profile = settingsStore.profiles.find((p) => p.uid === props.profileUID)
    if (!profile) return
    profile.member_profile_uids = [...selectedUIDs.value]
    await settingsStore.updateProfile(props.profileUID)
    popRef.value?.hide()
    emit('changed')
}

const title = computed(() =>
    isOverlay.value ? t('views.controls.switchBaseProfile') : t('views.controls.switchMembers'),
)

defineExpose({ toggle })
</script>

<template>
    <Popover ref="popRef" append-to="body">
        <div class="w-64 rounded-lg border border-border-one bg-bg-two p-3">
            <div class="mb-2 text-sm font-semibold text-text-color">
                {{ title }}
            </div>
            <div class="max-h-60 overflow-y-auto">
                <div
                    v-for="profile in eligibleProfiles"
                    :key="profile.uid"
                    class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 transition-colors hover:bg-surface-hover"
                    :class="{
                        'bg-accent/10': isOverlay
                            ? currentMemberUIDs?.[0] === profile.uid
                            : selectedUIDs.has(profile.uid),
                    }"
                    @click="isOverlay ? selectBase(profile.uid) : toggleMember(profile.uid)"
                >
                    <svg-icon
                        v-if="
                            isOverlay
                                ? currentMemberUIDs?.[0] === profile.uid
                                : selectedUIDs.has(profile.uid)
                        "
                        type="mdi"
                        :path="mdiCheck"
                        class="size-4 shrink-0 text-accent"
                    />
                    <div v-else class="size-4 shrink-0" />
                    <span class="flex-1 truncate text-sm text-text-color">
                        {{ profile.name }}
                    </span>
                    <span
                        class="rounded px-1.5 py-0.5 text-[10px] font-medium"
                        :class="typeBadgeClass[profile.p_type] ?? 'bg-info/20 text-info'"
                    >
                        {{ profile.p_type }}
                    </span>
                </div>
            </div>
            <div v-if="!isOverlay" class="mt-2 border-t border-border-one pt-2">
                <Button
                    :label="t('common.apply')"
                    class="w-full !bg-accent/80 !text-white hover:!bg-accent"
                    size="small"
                    :disabled="selectedUIDs.size < 2"
                    @click="applyMembers"
                />
            </div>
        </div>
    </Popover>
</template>
