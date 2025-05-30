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
import { inject, ref, type Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import { UID } from '@/models/Device.ts'
import NewProfile from '@/components/wizards/profile/NewProfile.vue'
import NewFixedProfile from '@/components/wizards/profile/NewFixedProfile.vue'
import NewGraphProfileTempSource from '@/components/wizards/fan-control/NewGraphProfileTempSource.vue'
import NewMixProfile from '@/components/wizards/profile/NewMixProfile.vue'
import {
    ProfileTempSource,
    ProfileType,
    Function,
    ProfileMixFunctionType,
} from '@/models/Profile.ts'
import NewGraphProfileSpeeds from '@/components/wizards/fan-control/NewGraphProfileSpeeds.vue'
import NewFunction from '@/components/wizards/fan-control/NewFunction.vue'
import ChooseFunctionAction from '@/components/wizards/fan-control/ChooseFunctionAction.vue'
import ExistingFunction from '@/components/wizards/fan-control/ExistingFunction.vue'
import Summary from '@/components/wizards/profile/Summary.vue'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const closeDialog = () => {
    dialogRef.value.close()
}

const currentStep: Ref<number> = ref(3)
const newProfileName: Ref<string> = ref('')
const newProfileType: Ref<ProfileType> = ref(ProfileType.Graph)
const newSpeedFixed: Ref<number | undefined> = ref()
const newMemberProfileIds: Ref<Array<UID>> = ref([])
const newMixFunctionType: Ref<ProfileMixFunctionType | undefined> = ref()
const newTempSource: Ref<ProfileTempSource | undefined> = ref()
const newSpeedProfile: Ref<Array<[number, number]>> = ref([])
const selectedFunctionUID: Ref<UID> = ref('0') // Default Function as default
const newFunction: Ref<Function | undefined> = ref()

const setFunctionUID = (funUID: UID): void => {
    selectedFunctionUID.value = funUID
    newFunction.value = undefined
}
</script>

<template>
    <NewProfile
        v-if="currentStep === 3"
        @next-step="(step: number) => (currentStep = step)"
        @profile-name="(name: string) => (newProfileName = name)"
        @profile-type="(type: ProfileType) => (newProfileType = type)"
        @close="closeDialog"
        :name="newProfileName"
        :type="newProfileType"
    />
    <NewFixedProfile
        v-else-if="currentStep === 6"
        @next-step="(step: number) => (currentStep = step)"
        @speed-fixed="(speed: number) => (newSpeedFixed = speed)"
        :name="newProfileName"
    />
    <NewMixProfile
        v-else-if="currentStep === 7"
        @next-step="(step: number) => (currentStep = step)"
        @member-ids="(memberIds: Array<UID>) => (newMemberProfileIds = memberIds)"
        @mix-function="(mixFunction: ProfileMixFunctionType) => (newMixFunctionType = mixFunction)"
        :name="newProfileName"
    />
    <NewGraphProfileTempSource
        v-else-if="currentStep === 8"
        @next-step="(step: number) => (currentStep = step)"
        @temp-source="(tempSource: ProfileTempSource) => (newTempSource = tempSource)"
        :name="newProfileName"
        :temp-source="newTempSource"
    />
    <NewGraphProfileSpeeds
        v-else-if="currentStep === 9"
        @next-step="(step: number) => (currentStep = step)"
        @speed-profile="(speedProfile: Array<[number, number]>) => (newSpeedProfile = speedProfile)"
        :name="newProfileName"
        :temp-source="newTempSource!"
        :speed-profile="newSpeedProfile"
    />
    <ChooseFunctionAction
        v-else-if="currentStep === 10"
        @next-step="(step: number) => (currentStep = step)"
        @function-u-i-d="setFunctionUID"
        :name="newProfileName"
    />
    <NewFunction
        v-else-if="currentStep === 11"
        @next-step="(step: number) => (currentStep = step)"
        @new-function="(fun: Function) => (newFunction = fun)"
        :profile-name="newProfileName"
    />
    <ExistingFunction
        v-else-if="currentStep === 12"
        @next-step="(step: number) => (currentStep = step)"
        @function-u-i-d="setFunctionUID"
        :selected-function-u-i-d="selectedFunctionUID"
    />
    <Summary
        v-else-if="currentStep === 13"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :name="newProfileName"
        :type="newProfileType"
        :speed-fixed="newSpeedFixed"
        :member-ids="newMemberProfileIds"
        :mix-function="newMixFunctionType"
        :temp-source="newTempSource"
        :speed-profile="newSpeedProfile"
        :function-u-i-d="selectedFunctionUID"
        :new-function="newFunction"
    />
</template>

<style scoped lang="scss"></style>
