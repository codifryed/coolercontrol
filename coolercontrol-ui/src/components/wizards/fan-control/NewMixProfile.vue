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
import { mdiArrowLeft, mdiContentSaveOutline } from '@mdi/js'
import { UID } from '@/models/Device.ts'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useRouter } from 'vue-router'
import { DeviceSettingWriteProfileDTO } from '@/models/DaemonSettings.ts'
import { v4 as uuidV4 } from 'uuid'
import {
    getProfileMixFunctionTypeDisplayName,
    Profile,
    ProfileMixFunctionType,
    ProfileType,
} from '@/models/Profile.ts'
import { useToast } from 'primevue/usetoast'
import { computed, inject, ref, Ref } from 'vue'
import MultiSelect from 'primevue/multiselect'
import Select from 'primevue/select'
import { $enum } from 'ts-enum-util'
import { Emitter, EventType } from 'mitt'
import Button from 'primevue/button'

interface Props {
    deviceUID: UID
    channelName: string
    name: string
}

const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()
const props = defineProps<Props>()

const { t } = useI18n()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const toast = useToast()
const router = useRouter()

const channelLabel =
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)
        ?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName

const chosenProfileMixFunction: Ref<ProfileMixFunctionType> = ref(ProfileMixFunctionType.Max)
const mixFunctionTypeOptions = computed(() => {
    return [...$enum(ProfileMixFunctionType).values()].map((type) => ({
        value: type,
        label: getProfileMixFunctionTypeDisplayName(type),
    }))
})
const chosenMemberProfiles: Ref<Array<Profile>> = ref([])
const memberProfileOptions: Ref<Array<Profile>> = computed(() =>
    settingsStore.profiles.filter((profile) => profile.p_type === ProfileType.Graph),
)
const saveSetting = async () => {
    if (chosenMemberProfiles.value.length < 2) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('views.profiles.memberProfilesRequired'),
            life: 4000,
        })
        return
    }
    const newProfile = new Profile(props.name, ProfileType.Mix)
    newProfile.member_profile_uids = chosenMemberProfiles.value.map((p) => p.uid)
    newProfile.mix_function_type = chosenProfileMixFunction.value
    settingsStore.profiles.push(newProfile)
    await settingsStore.saveProfile(newProfile.uid)
    emitter.emit('profile-add-menu', { profileUID: newProfile.uid })
    const setting = new DeviceSettingWriteProfileDTO(newProfile.uid)
    await settingsStore.saveDaemonDeviceSettingProfile(props.deviceUID, props.channelName, setting)
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('components.wizards.fanControl.profileCreatedApplied'),
        life: 3000,
    })
    emit('close')
    await router.push({
        name: 'device-speed',
        params: { deviceUID: props.deviceUID, channelName: props.channelName },
        query: { key: uuidV4() },
    })
}
</script>

<template>
    <div class="flex flex-col gap-y-4 w-96">
        <div class="w-full text-lg">
            {{ t('components.wizards.fanControl.newMixProfile') }}:
            <span class="font-bold">{{ props.name }}</span
            ><br />
            {{ t('components.wizards.fanControl.willCreatedAndAppliedTo') }}
            <span class="font-bold">{{ channelLabel }}</span
            >:
        </div>
        <div class="mt-0 flex flex-col">
            <small class="ml-2 mb-1 font-light text-sm text-text-color-secondary">
                {{ t('views.profiles.applyMixFunction') }}
            </small>
            <Select
                v-model="chosenProfileMixFunction"
                :options="mixFunctionTypeOptions"
                option-label="label"
                option-value="value"
                class="w-full mr-3 bg-bg-two"
                checkmark
                dropdown-icon="pi pi-sliders-v"
                scroll-height="40rem"
            />
        </div>
        <div class="mt-0 flex flex-col">
            <small class="ml-2 mb-1 font-light text-sm text-text-color-secondary">
                {{ t('views.profiles.profilesToMix') }}
            </small>
            <MultiSelect
                v-model="chosenMemberProfiles"
                :options="memberProfileOptions"
                option-label="name"
                :placeholder="t('views.profiles.memberProfiles')"
                class="w-full bg-bg-two"
                scroll-height="40rem"
                dropdown-icon="pi pi-chart-line"
                :invalid="chosenMemberProfiles.length < 2"
            />
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 h-[2.375rem]" label="Back" @click="emit('nextStep', 3)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                :label="t('common.apply')"
                v-tooltip.bottom="t('views.speed.applySetting')"
                :disabled="chosenMemberProfiles.length < 2"
                @click="saveSetting"
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
