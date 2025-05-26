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
import Select from 'primevue/select'
import Button from 'primevue/button'
import { computed, ref, type Ref } from 'vue'
import { getProfileTypeDisplayName, ProfileType } from '@/models/Profile.ts'
import { $enum } from 'ts-enum-util'
import { DEFAULT_NAME_STRING_LENGTH, useDeviceStore } from '@/stores/DeviceStore.ts'
import InputText from 'primevue/inputtext'
import { useI18n } from 'vue-i18n'

interface Props {
    name: string
    type: ProfileType
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'profileName', name: string): void
    (e: 'profileType', type: ProfileType): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()

const selectedType: Ref<ProfileType> = ref(props.type)
const profileTypeOptions = computed(() => {
    return [...$enum(ProfileType).values()].map((type) => ({
        value: type,
        label: getProfileTypeDisplayName(type),
    }))
})
const nameInput: Ref<string> = ref(props.name)
const nameInvalid = computed(() => {
    return nameInput.value.length < 1 || nameInput.value.length > DEFAULT_NAME_STRING_LENGTH
})

const nextStep = () => {
    emit('profileName', nameInput.value)
    emit('profileType', selectedType.value)
    switch (selectedType.value) {
        case ProfileType.Default:
            emit('nextStep', 13)
            break
        case ProfileType.Fixed:
            emit('nextStep', 6)
            break
        case ProfileType.Mix:
            emit('nextStep', 7)
            break
        case ProfileType.Graph:
            emit('nextStep', 8)
            break
    }
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div class="w-full mb-2">
                {{ t('components.wizards.fanControl.chooseProfileNameType') }}:
            </div>
            <div class="mt-0 flex flex-col">
                <InputText
                    v-model="nameInput"
                    :placeholder="t('common.name')"
                    ref="inputArea"
                    id="property-name"
                    class="w-full h-11"
                    :invalid="nameInvalid"
                    :input-style="{ background: 'rgb(var(--color-bg-one))' }"
                />
            </div>
            <div class="mt-0 flex flex-col">
                <small class="ml-2 mb-1 font-light text-sm">
                    {{ t('views.profiles.profileType') }}
                </small>
                <Select
                    v-model="selectedType"
                    :options="profileTypeOptions"
                    option-label="label"
                    option-value="value"
                    :placeholder="t('views.profiles.profileType')"
                    class="w-full h-11 mr-3 bg-bg-one !justify-end"
                    dropdown-icon="pi pi-chart-line"
                    scroll-height="400px"
                    checkmark
                />
            </div>
            <p>
                <span v-html="t('views.profiles.tooltip.profileType')" />
            </p>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <div />
            <Button
                class="w-24 bg-bg-one"
                :label="t('common.next')"
                :disabled="nameInvalid"
                @click="nextStep"
            />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
