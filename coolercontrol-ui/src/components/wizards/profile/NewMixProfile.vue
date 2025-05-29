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
import { mdiArrowLeft } from '@mdi/js'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import {
    getProfileMixFunctionTypeDisplayName,
    Profile,
    ProfileMixFunctionType,
    ProfileType,
} from '@/models/Profile.ts'
import { useToast } from 'primevue/usetoast'
import { computed, ref, Ref } from 'vue'
import MultiSelect from 'primevue/multiselect'
import Select from 'primevue/select'
import { $enum } from 'ts-enum-util'
import Button from 'primevue/button'
import { UID } from '@/models/Device.ts'

interface Props {
    name: string
}

const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'memberIds', memberIds: Array<UID>): void
    (e: 'mixFunction', mixFunction: ProfileMixFunctionType): void
}>()
const props = defineProps<Props>()

const { t } = useI18n()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const toast = useToast()

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
const nextStep = async () => {
    if (chosenMemberProfiles.value.length < 2) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('views.profiles.memberProfilesRequired'),
            life: 4000,
        })
        return
    }
    emit(
        'memberIds',
        chosenMemberProfiles.value.map((p) => p.uid),
    )
    emit('mixFunction', chosenProfileMixFunction.value)
    emit('nextStep', 13)
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div class="w-full text-lg">
                {{ t('components.wizards.fanControl.newMixProfile') }}:
                <span class="font-bold">{{ props.name }}</span
                ><br /><br />{{ t('components.wizards.fanControl.withSettings') }}:
            </div>
            <div class="mt-0 flex flex-col">
                <small class="ml-2 mb-1 font-light text-sm">
                    {{ t('views.profiles.profilesToMix') }}
                </small>
                <MultiSelect
                    v-model="chosenMemberProfiles"
                    :options="memberProfileOptions"
                    option-label="name"
                    :placeholder="t('views.profiles.memberProfiles')"
                    class="w-full h-11 bg-bg-one items-center"
                    scroll-height="40rem"
                    dropdown-icon="pi pi-chart-line"
                    :invalid="chosenMemberProfiles.length < 2"
                />
            </div>
            <div class="mt-0 flex flex-col">
                <small class="ml-2 mb-1 font-light text-sm">
                    {{ t('views.profiles.applyMixFunction') }}
                </small>
                <Select
                    v-model="chosenProfileMixFunction"
                    :options="mixFunctionTypeOptions"
                    option-label="label"
                    option-value="value"
                    class="w-full mr-3 h-11 bg-bg-one !justify-end"
                    checkmark
                    dropdown-icon="pi pi-sliders-v"
                    scroll-height="40rem"
                />
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 3)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="w-24 bg-bg-one"
                :label="t('common.next')"
                :disabled="chosenMemberProfiles.length < 2"
                @click="nextStep"
            />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
