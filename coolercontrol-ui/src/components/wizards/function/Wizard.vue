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
import { Function } from '@/models/Profile.ts'
import NewFunction from '@/components/wizards/fan-control/NewFunction.vue'
import Summary from '@/components/wizards/function/Summary.vue'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const closeDialog = () => {
    dialogRef.value.close()
}

const currentStep: Ref<number> = ref(11)
const newFunction: Ref<Function | undefined> = ref()
</script>

<template>
    <NewFunction
        v-if="currentStep === 11"
        @next-step="(step: number) => (currentStep = step)"
        @new-function="(fun: Function) => (newFunction = fun)"
        :function-name="''"
        :new-function="newFunction"
    />
    <Summary
        v-else-if="currentStep === 13"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :new-function="newFunction ?? Function.createDefault()"
    />
</template>

<style scoped lang="scss"></style>
