<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">

import {inject, ref, type Ref} from "vue"
import {type DynamicDialogInstance} from "primevue/dynamicdialogoptions"
import InputText from 'primevue/inputtext'
import {useSettingsStore} from "@/stores/SettingsStore"
import Button from 'primevue/button'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {mdiContentSaveMoveOutline} from "@mdi/js";

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const deviceSettings = useSettingsStore().allUIDeviceSettings.get(dialogRef.value.data.deviceUID)!
const sensorName: string | undefined = dialogRef.value.data.sensorName
const isDeviceName: boolean = sensorName == null
const currentName: string = isDeviceName
    ? deviceSettings.name
    : deviceSettings.sensorsAndChannels.getValue(sensorName!).name
const isUserName: boolean = isDeviceName
    ? deviceSettings.userName != null
    : deviceSettings.sensorsAndChannels.getValue(sensorName!).userName != null
const nameInput: Ref<string> = ref(isUserName ? currentName : '')
const systemDisplayName = isDeviceName
    ? deviceSettings.displayName
    : deviceSettings.sensorsAndChannels.getValue(sensorName!).displayName
const closeAndSave = (): void => {
  dialogRef.value.close({newName: nameInput.value})
}

</script>

<template>
  <span class="p-float-label mt-4">
    <InputText id="property-name" class="w-20rem" v-model="nameInput"/>
    <label for="property-name">{{ systemDisplayName }}</label>
  </span>
  <small id="rename-help">A blank name will reset it to the system default.</small>
  <br/>
  <footer class="text-right mt-4">
    <Button label="Save" @click="closeAndSave" rounded>
      <svg-icon class="p-button-icon p-button-icon-left pi" type="mdi" :path="mdiContentSaveMoveOutline"
                size="1.35rem"/>
      <span class="p-button-label">Save</span>
    </Button>
  </footer>
</template>

<style scoped lang="scss">

</style>