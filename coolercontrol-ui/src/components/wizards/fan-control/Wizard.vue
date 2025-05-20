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
import ChooseInitialAction from '@/components/wizards/fan-control/ChooseInitialAction.vue'
import ExistingProfile from '@/components/wizards/fan-control/ExistingProfile.vue'
import NewProfile from '@/components/wizards/fan-control/NewProfile.vue'
import Manual from '@/components/wizards/fan-control/Manual.vue'
import NewDefaultProfile from '@/components/wizards/fan-control/NewDefaultProfile.vue'
import NewFixedProfile from '@/components/wizards/fan-control/NewFixedProfile.vue'
import NewGraphProfileTempSource from '@/components/wizards/fan-control/NewGraphProfileTempSource.vue'
import NewMixProfile from '@/components/wizards/fan-control/NewMixProfile.vue'
import { ProfileTempSource, ProfileType, Function } from '@/models/Profile.ts'
import NewGraphProfileSpeeds from '@/components/wizards/fan-control/NewGraphProfileSpeeds.vue'
import NewFunction from '@/components/wizards/fan-control/NewFunction.vue'
import ChooseFunctionAction from '@/components/wizards/fan-control/ChooseFunctionAction.vue'
import ExistingFunction from '@/components/wizards/fan-control/ExistingFunction.vue'
import Summary from '@/components/wizards/fan-control/Summary.vue'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const closeDialog = () => {
    dialogRef.value.close()
}

const deviceUID: UID = dialogRef.value.data.deviceUID
const channelName: string = dialogRef.value.data.channelName
const selectedProfileUID: Ref<UID | undefined> = ref(dialogRef.value.data.selectedProfileUID)
const currentStep: Ref<number> = ref(1)
const newProfileName: Ref<string> = ref('')
const newProfileType: Ref<ProfileType> = ref(ProfileType.Graph)
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
    <ChooseInitialAction
        v-if="currentStep === 1"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
        :selected-profile-u-i-d="selectedProfileUID"
    />
    <ExistingProfile
        v-else-if="currentStep === 2"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
        :selected-profile-u-i-d="selectedProfileUID"
    />
    <NewProfile
        v-else-if="currentStep === 3"
        @next-step="(step: number) => (currentStep = step)"
        @profile-name="(name: string) => (newProfileName = name)"
        @profile-type="(type: ProfileType) => (newProfileType = type)"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
        :name="newProfileName"
        :type="newProfileType"
    />
    <Manual
        v-else-if="currentStep === 4"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
    />
    <NewDefaultProfile
        v-else-if="currentStep === 5"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
        :name="newProfileName"
    />
    <NewFixedProfile
        v-else-if="currentStep === 6"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
        :name="newProfileName"
    />
    <NewMixProfile
        v-else-if="currentStep === 7"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
        :name="newProfileName"
    />
    <NewGraphProfileTempSource
        v-else-if="currentStep === 8"
        @next-step="(step: number) => (currentStep = step)"
        @temp-source="(tempSource: ProfileTempSource) => (newTempSource = tempSource)"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
        :name="newProfileName"
        :temp-source="newTempSource"
    />
    <NewGraphProfileSpeeds
        v-else-if="currentStep === 9"
        @next-step="(step: number) => (currentStep = step)"
        @speed-profile="(speedProfile: Array<[number, number]>) => (newSpeedProfile = speedProfile)"
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
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
        :name="newProfileName"
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
        :device-u-i-d="deviceUID"
        :channel-name="channelName"
        :name="newProfileName"
        :temp-source="newTempSource!"
        :speed-profile="newSpeedProfile"
        :function-u-i-d="selectedFunctionUID"
        :new-function="newFunction"
    />
</template>

<style scoped lang="scss"></style>
