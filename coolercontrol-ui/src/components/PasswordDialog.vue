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
import { inject, nextTick, ref, type Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import Password from 'primevue/password'
import Button from 'primevue/button'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiHelpCircleOutline } from '@mdi/js'
import FloatLabel from 'primevue/floatlabel'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useI18n } from 'vue-i18n'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const deviceStore = useDeviceStore()
const { t } = useI18n()

const setPasswd: boolean = dialogRef.value.data.setPasswd
const promptMessage: string | undefined = dialogRef.value.data.promptMessage
const currentPasswdInput: Ref<string> = ref(dialogRef.value.data.currentPasswd || '')
const passwdInput: Ref<string> = ref('')

const closeAndProcess = (): void => {
    if (formIsInvalid()) {
        return
    }
    if (setPasswd) {
        dialogRef.value.close({
            currentPasswd: currentPasswdInput.value,
            passwd: passwdInput.value,
        })
    } else {
        dialogRef.value.close({ passwd: passwdInput.value })
    }
}
const passwordIsInvalid = (value: string): boolean => value == null || value.trim().length === 0
const formIsInvalid = (): boolean => {
    if (setPasswd) {
        return passwordIsInvalid(currentPasswdInput.value) || passwordIsInvalid(passwdInput.value)
    }
    return passwordIsInvalid(passwdInput.value)
}
const currentPasswdInputArea = ref()
const passwdInputArea = ref()

nextTick(async () => {
    const delay = () => new Promise((resolve) => setTimeout(resolve, 300))
    await delay()
    if (setPasswd && !dialogRef.value.data.currentPasswd) {
        currentPasswdInputArea.value.$el.children[0].focus()
    } else if (setPasswd && dialogRef.value.data.currentPasswd) {
        passwdInputArea.value.$el.children[0].focus()
    } else {
        passwdInputArea.value.$el.children[0].focus()
    }
})
</script>

<template>
    <p v-if="promptMessage" class="mb-12 text-text-color whitespace-pre-line">
        {{ promptMessage }}
    </p>
    <FloatLabel v-if="setPasswd" class="mt-6">
        <Password
            ref="currentPasswdInputArea"
            :class="{ filled: !passwordIsInvalid(currentPasswdInput) }"
            id="current-password"
            v-model="currentPasswdInput"
            :invalid="passwordIsInvalid(currentPasswdInput)"
            :feedback="false"
            toggle-mask
            required
            @keydown.enter="closeAndProcess"
            autofocus
        />
        <label for="current-password">{{ t('common.currentPassword') }}</label>
    </FloatLabel>
    <FloatLabel class="mt-6 mb-24">
        <Password
            ref="passwdInputArea"
            :class="{ filled: !passwordIsInvalid(passwdInput) }"
            :id="setPasswd ? 'new-password' : 'password'"
            v-model="passwdInput"
            :invalid="passwordIsInvalid(passwdInput)"
            :feedback="setPasswd"
            toggle-mask
            required
            @keydown.enter="closeAndProcess"
            autofocus
            :prompt-label="t('common.passwordPrompt')"
            :weak-label="t('common.passwordWeak')"
            :medium-label="t('common.passwordMedium')"
            :strong-label="t('common.passwordStrong')"
        />
        <label :for="setPasswd ? 'new-password' : 'password'">{{
            setPasswd ? t('common.newPassword') : t('common.password')
        }}</label>
    </FloatLabel>
    <footer class="flex items-center place-content-between mt-4">
        <Button
            class="!p-0 rounded-lg w-8 h-8"
            link
            v-tooltip.bottom="{
                value: t('components.password.passwordHelp'),
                autoHide: false,
                escape: false,
                hideDelay: 500,
            }"
        >
            <svg-icon
                class="text-text-color-secondary"
                type="mdi"
                :path="mdiHelpCircleOutline"
                :size="deviceStore.getREMSize(1.1)"
            />
        </Button>
        <Button
            class="bg-accent/80 hover:!bg-accent/100"
            label="Save"
            @click="closeAndProcess"
            :disabled="formIsInvalid()"
        >
            {{ setPasswd ? t('common.savePassword') : t('common.ok') }}
        </Button>
    </footer>
</template>

<style scoped lang="scss"></style>
