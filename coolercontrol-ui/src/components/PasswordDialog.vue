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
import Password from 'primevue/password'
import Button from 'primevue/button'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiHelpCircleOutline } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const deviceStore = useDeviceStore()

const setPasswd: boolean = dialogRef.value.data.setPasswd
const passwdInput: Ref<string> = ref('')

const closeAndProcess = (): void => {
    if (passwordIsInvalid()) {
        return
    }
    dialogRef.value.close({ passwd: passwdInput.value })
}
const passwordIsInvalid = (): boolean =>
    passwdInput.value == null || passwdInput.value.trim().length === 0
const passwdInputArea = ref()

nextTick(async () => {
    const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
    await delay()
    passwdInputArea.value.$el.children[0].focus()
})
</script>

<template>
    <div class="flex w-full justify-content-end mt-2">
        <Button
            link
            v-tooltip.top="{
                value:
                    'Upon installation the daemon uses a default password to protect device control endpoints. ' +
                    'Optionally you can create a strong password for improved protection. ' +
                    'If you see this dialog and have not yet set a password, try refreshing the UI ' +
                    ' or clicking on Login from the Access Protection menu. See the the project wiki for more information.',
                autoHide: false,
            }"
            class="p-0 mr-2 vertical-align-bottom"
        >
            <svg-icon type="mdi" :path="mdiHelpCircleOutline" :size="deviceStore.getREMSize(1.1)" />
        </Button>
    </div>
    <span class="p-float-label mt-2">
        <Password
            ref="passwdInputArea"
            id="property-name"
            v-model="passwdInput"
            :feedback="false"
            toggle-mask
            required
            :inputProps="{ autocomplete: 'true' }"
            @keydown.enter="closeAndProcess"
        />
        <label for="property-name">Password</label>
    </span>
    <footer class="text-right mt-4">
        <Button label="Save" @click="closeAndProcess" rounded :disabled="passwordIsInvalid()">
            <span class="p-button-label">{{ setPasswd ? 'Save Password' : 'Ok' }}</span>
        </Button>
    </footer>
</template>

<style scoped lang="scss"></style>
