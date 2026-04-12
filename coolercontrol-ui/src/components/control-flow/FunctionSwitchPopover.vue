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
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Popover from 'primevue/popover'
import { useSettingsStore } from '@/stores/SettingsStore'
import type { UID } from '@/models/Device'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiCheck } from '@mdi/js'

const props = defineProps<{
    profileUID: UID
    currentFunctionUID?: string
}>()

const emit = defineEmits<{
    (e: 'changed'): void
}>()

const { t } = useI18n()
const settingsStore = useSettingsStore()
const popRef = ref()

const typeBadgeClass: Record<string, string> = {
    Identity: 'bg-info/20 text-info',
    Standard: 'bg-success/20 text-success',
    ExponentialMovingAvg: 'bg-accent/20 text-accent',
}

function toggle(event: Event) {
    popRef.value?.toggle(event)
}

async function selectFunction(functionUID: UID) {
    if (functionUID === (props.currentFunctionUID ?? '0')) {
        popRef.value?.hide()
        return
    }
    const profile = settingsStore.profiles.find((p) => p.uid === props.profileUID)
    if (!profile) return
    profile.function_uid = functionUID
    await settingsStore.updateProfile(props.profileUID)
    popRef.value?.hide()
    emit('changed')
}

defineExpose({ toggle })
</script>

<template>
    <Popover ref="popRef" append-to="body">
        <div class="w-64 rounded-lg border border-border-one bg-bg-two p-3">
            <div class="mb-2 text-sm font-semibold text-text-color">
                {{ t('views.controls.switchFunction') }}
            </div>
            <div class="max-h-60 overflow-y-auto">
                <div
                    v-for="fn in settingsStore.functions"
                    :key="fn.uid"
                    class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 transition-colors hover:bg-surface-hover"
                    :class="fn.uid === (currentFunctionUID ?? '0') ? 'bg-accent/10' : ''"
                    @click="selectFunction(fn.uid)"
                >
                    <svg-icon
                        v-if="fn.uid === (currentFunctionUID ?? '0')"
                        type="mdi"
                        :path="mdiCheck"
                        class="size-4 shrink-0 text-accent"
                    />
                    <div v-else class="size-4 shrink-0" />
                    <span class="flex-1 truncate text-sm text-text-color">
                        {{ fn.name }}
                    </span>
                    <span
                        class="rounded px-1.5 py-0.5 text-[10px] font-medium"
                        :class="typeBadgeClass[fn.f_type] ?? 'bg-info/20 text-info'"
                    >
                        {{ fn.f_type }}
                    </span>
                </div>
            </div>
        </div>
    </Popover>
</template>
