<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2024  Guy Boldon and contributors
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
import { inject, nextTick, ref, type Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import InputText from 'primevue/inputtext'
import { useSettingsStore } from '@/stores/SettingsStore'
import Button from 'primevue/button'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const inputArea = ref()
const deviceSettings = useSettingsStore().allUIDeviceSettings.get(dialogRef.value.data.deviceUID)!
const sensorName: string | undefined = dialogRef.value.data.sensorName
const isDeviceName: boolean = sensorName == null
const currentName: string = isDeviceName
    ? deviceSettings.name
    : deviceSettings.sensorsAndChannels.get(sensorName!)!.name
const isUserName: boolean = isDeviceName
    ? deviceSettings.userName != null
    : deviceSettings.sensorsAndChannels.get(sensorName!)!.userName != null
const nameInput: Ref<string> = ref(isUserName ? currentName : '')
const systemDisplayName = isDeviceName
    ? deviceSettings.displayName
    : deviceSettings.sensorsAndChannels.get(sensorName!)!.displayName
const closeAndSave = (): void => {
    dialogRef.value.close({ newName: nameInput.value })
}

nextTick(async () => {
    const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
    await delay()
    inputArea.value.$el.focus()
})
</script>

<template>
    <span class="p-float-label mt-4">
        <InputText
            ref="inputArea"
            id="property-name"
            class="w-20rem"
            v-model="nameInput"
            @keydown.enter="closeAndSave"
        />
        <label for="property-name">{{ systemDisplayName }}</label>
    </span>
    <small id="rename-help">A blank name will reset it to the system default.</small>
    <br />
    <footer class="text-right mt-4">
        <Button label="Save" @click="closeAndSave" rounded>
            <span class="p-button-label">Save</span>
        </Button>
    </footer>
</template>

<style scoped lang="scss"></style>
