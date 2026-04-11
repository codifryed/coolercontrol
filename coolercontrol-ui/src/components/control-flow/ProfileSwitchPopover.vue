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
import { defineAsyncComponent, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDialog } from 'primevue/usedialog'
import Popover from 'primevue/popover'
import { useSettingsStore } from '@/stores/SettingsStore'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings'
import { ProfileType } from '@/models/Profile'
import type { UID } from '@/models/Device'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiCheck, mdiGauge, mdiPlusBoxOutline } from '@mdi/js'

const props = defineProps<{
    deviceUID: UID
    channelName: string
    currentProfileUID?: UID
}>()

const emit = defineEmits<{
    (e: 'profile-switched'): void
}>()

const { t } = useI18n()
const settingsStore = useSettingsStore()
const dialog = useDialog()
const popRef = ref()

const fanControlWizard = defineAsyncComponent(
    () => import('@/components/wizards/fan-control/Wizard.vue'),
)

const typeBadgeClass: Record<string, string> = {
    [ProfileType.Default]: 'bg-info/20 text-info',
    [ProfileType.Fixed]: 'bg-success/20 text-success',
    [ProfileType.Graph]: 'bg-accent/20 text-accent',
    [ProfileType.Mix]: 'bg-pink/20 text-pink',
    [ProfileType.Overlay]: 'bg-warning/20 text-warning',
}

function toggle(event: Event) {
    popRef.value?.toggle(event)
}

async function selectProfile(profileUID: UID) {
    if (profileUID === props.currentProfileUID) {
        popRef.value?.hide()
        return
    }
    const setting = new DeviceSettingWriteProfileDTO(profileUID)
    await settingsStore.saveDaemonDeviceSettingProfile(props.deviceUID, props.channelName, setting)
    popRef.value?.hide()
    emit('profile-switched')
}

function openWizardAt(step: number) {
    popRef.value?.hide()
    dialog.open(fanControlWizard, {
        props: {
            header: t('components.wizards.fanControl.fanControlWizard'),
            position: 'center',
            modal: true,
            dismissableMask: true,
        },
        data: {
            deviceUID: props.deviceUID,
            channelName: props.channelName,
            selectedProfileUID: props.currentProfileUID,
            isControlFlowView: true,
            initialStep: step,
        },
        onClose: () => {
            emit('profile-switched')
        },
    })
}

defineExpose({ toggle })
</script>

<template>
    <Popover ref="popRef" append-to="body">
        <div class="w-64 rounded-lg border border-border-one bg-bg-two p-3">
            <div class="mb-2 text-sm font-semibold text-text-color">
                {{ t('views.controls.switchProfile') }}
            </div>
            <div class="max-h-60 overflow-y-auto">
                <div
                    v-for="profile in settingsStore.profiles"
                    :key="profile.uid"
                    class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 transition-colors hover:bg-surface-hover"
                    :class="profile.uid === currentProfileUID ? 'bg-accent/10' : ''"
                    @click="selectProfile(profile.uid)"
                >
                    <svg-icon
                        v-if="profile.uid === currentProfileUID"
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
            <div class="mt-2 border-t border-border-one pt-2">
                <div
                    class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-accent transition-colors hover:bg-surface-hover"
                    @click="openWizardAt(3)"
                >
                    <svg-icon type="mdi" :path="mdiPlusBoxOutline" class="size-4" />
                    <span class="text-sm font-medium">
                        {{ t('components.wizards.fanControl.createNewProfile') }}...
                    </span>
                </div>
                <div
                    class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-text-color-secondary transition-colors hover:bg-surface-hover"
                    @click="openWizardAt(4)"
                >
                    <svg-icon type="mdi" :path="mdiGauge" class="size-4" />
                    <span class="text-sm font-medium">
                        {{ t('components.wizards.fanControl.manualSpeed') }}
                    </span>
                </div>
            </div>
        </div>
    </Popover>
</template>
