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
import NewAlert from '@/components/wizards/alert/NewAlert.vue'
import Summary from '@/components/wizards/alert/Summary.vue'
import { Alert } from '@/models/Alert.ts'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const closeDialog = () => {
    dialogRef.value.close()
}

const currentStep: Ref<number> = ref(1)
const newAlert: Ref<Alert | undefined> = ref()
</script>

<template>
    <NewAlert
        v-if="currentStep === 1"
        @next-step="(step: number) => (currentStep = step)"
        @new-alert="(alert: Alert) => (newAlert = alert)"
        :alert="newAlert"
    />
    <Summary
        v-else-if="currentStep === 2"
        @next-step="(step: number) => (currentStep = step)"
        @close="closeDialog"
        :alert="newAlert!"
    />
</template>

<style scoped lang="scss"></style>
