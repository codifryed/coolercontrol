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
import { inject, nextTick, ref, watch, type Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import Password from 'primevue/password'
import Button from 'primevue/button'
import FloatLabel from 'primevue/floatlabel'
import { useI18n } from 'vue-i18n'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const { t } = useI18n()

const setPasswd: boolean = dialogRef.value.data.setPasswd
const promptMessage: string | undefined = dialogRef.value.data.promptMessage
const autoFilledCurrentPasswd: boolean = !!dialogRef.value.data.currentPasswd
const currentPasswdInput: Ref<string> = ref(dialogRef.value.data.currentPasswd || '')
const passwdInput: Ref<string> = ref('')
const confirmPasswdInput: Ref<string> = ref('')

// Step 1 = verify current password, step 2 = set new password.
// Skip step 1 when auto-filled or in login mode.
const step: Ref<number> = ref(setPasswd && !autoFilledCurrentPasswd ? 1 : 2)

const submitError: Ref<string | null> = ref(null)
const submitting: Ref<boolean> = ref(false)

// Track which fields have been interacted with so we don't show errors on a fresh dialog.
const currentPasswdTouched = ref(autoFilledCurrentPasswd)
const passwdTouched = ref(false)
const confirmPasswdTouched = ref(false)
watch(currentPasswdInput, () => {
    currentPasswdTouched.value = true
})
watch(passwdInput, () => {
    passwdTouched.value = true
})
watch(confirmPasswdInput, () => {
    confirmPasswdTouched.value = true
})

const passwordIsInvalid = (value: string): boolean => value == null || value.trim().length === 0
const passwordsMismatch = (): boolean => setPasswd && passwdInput.value !== confirmPasswdInput.value
const formIsInvalid = (): boolean => {
    if (setPasswd) {
        return (
            passwordIsInvalid(currentPasswdInput.value) ||
            passwordIsInvalid(passwdInput.value) ||
            passwordIsInvalid(confirmPasswdInput.value) ||
            passwordsMismatch()
        )
    }
    return passwordIsInvalid(passwdInput.value)
}

const closeAndProcess = async (): Promise<void> => {
    // Force-touch all fields so validation icons appear if anything is wrong.
    currentPasswdTouched.value = true
    passwdTouched.value = true
    confirmPasswdTouched.value = true
    if (formIsInvalid()) {
        return
    }
    const onSubmit:
        | ((currentPasswd: string, passwd: string) => Promise<string | null>)
        | undefined = dialogRef.value.data.onSubmit
    if (onSubmit) {
        submitError.value = null
        submitting.value = true
        const error = await onSubmit(currentPasswdInput.value, passwdInput.value)
        submitting.value = false
        if (error != null) {
            submitError.value = error
            return
        }
        dialogRef.value.close({
            currentPasswd: currentPasswdInput.value,
            passwd: passwdInput.value,
        })
    } else {
        dialogRef.value.close({ passwd: passwdInput.value })
    }
}

const currentPasswdInputArea = ref()
const passwdInputArea = ref()
const confirmPasswdInputArea = ref()

const onVerifyCurrentPassword: ((currentPasswd: string) => Promise<string | null>) | undefined =
    dialogRef.value.data.onVerifyCurrentPassword

const goNext = async (): Promise<void> => {
    currentPasswdTouched.value = true
    if (passwordIsInvalid(currentPasswdInput.value)) return
    if (onVerifyCurrentPassword) {
        submitError.value = null
        submitting.value = true
        const error = await onVerifyCurrentPassword(currentPasswdInput.value)
        submitting.value = false
        if (error != null) {
            submitError.value = error
            return
        }
    }
    step.value = 2
    nextTick(() => passwdInputArea.value.$el.children[0].focus())
}

const goBack = (): void => {
    step.value = 1
    submitError.value = null
    nextTick(() => currentPasswdInputArea.value.$el.children[0].focus())
}

const focusConfirm = (): void => {
    if (passwordIsInvalid(passwdInput.value)) return
    nextTick(() => confirmPasswdInputArea.value.$el.children[0].focus())
}

nextTick(async () => {
    await new Promise((resolve) => setTimeout(resolve, 300))
    if (step.value === 1) {
        currentPasswdInputArea.value.$el.children[0].focus()
    } else {
        passwdInputArea.value.$el.children[0].focus()
    }
})
</script>

<template>
    <form @submit.prevent>
        <p v-if="promptMessage" class="mb-12 w-64 text-text-color whitespace-pre-line">
            {{ promptMessage }}
        </p>
        <div
            v-if="setPasswd && !autoFilledCurrentPasswd"
            class="text-text-color-secondary text-sm text-center mb-4"
        >
            {{ step }} / 2
        </div>

        <!-- Step 1: current password -->
        <template v-if="step === 1">
            <input type="text" autocomplete="username" value="CCAdmin" hidden aria-hidden="true" />
            <FloatLabel class="mt-6 mb-1">
                <Password
                    ref="currentPasswdInputArea"
                    :class="{ filled: !passwordIsInvalid(currentPasswdInput) }"
                    id="current-password"
                    v-model="currentPasswdInput"
                    :invalid="currentPasswdTouched && passwordIsInvalid(currentPasswdInput)"
                    :feedback="false"
                    :pt="{ pcInputText: { root: { autocomplete: 'current-password' } } }"
                    toggle-mask
                    required
                    @keydown.enter="goNext"
                />
                <label for="current-password">{{ t('common.currentPassword') }}</label>
            </FloatLabel>
            <div class="min-h-[1.2rem] flex items-center gap-1 mb-16">
                <i
                    v-if="currentPasswdTouched && passwordIsInvalid(currentPasswdInput)"
                    class="pi pi-times-circle text-red text-xs"
                />
            </div>
        </template>

        <!-- Step 2: new password + confirm -->
        <template v-if="step === 2">
            <input type="text" autocomplete="username" value="CCAdmin" hidden aria-hidden="true" />
            <FloatLabel class="mt-6 mb-1">
                <Password
                    ref="passwdInputArea"
                    :class="{ filled: !passwordIsInvalid(passwdInput) }"
                    :id="setPasswd ? 'new-password' : 'password'"
                    v-model="passwdInput"
                    :invalid="passwdTouched && passwordIsInvalid(passwdInput)"
                    :feedback="setPasswd"
                    :pt="{
                        pcInputText: {
                            root: { autocomplete: setPasswd ? 'new-password' : 'current-password' },
                        },
                    }"
                    toggle-mask
                    required
                    @keydown.enter="setPasswd ? focusConfirm() : closeAndProcess()"
                    :prompt-label="t('common.passwordPrompt')"
                    :weak-label="t('common.passwordWeak')"
                    :medium-label="t('common.passwordMedium')"
                    :strong-label="t('common.passwordStrong')"
                />
                <label :for="setPasswd ? 'new-password' : 'password'">{{
                    setPasswd ? t('common.newPassword') : t('common.password')
                }}</label>
            </FloatLabel>
            <div class="min-h-[1.2rem] flex items-center gap-1">
                <i
                    v-if="passwdTouched && passwordIsInvalid(passwdInput)"
                    class="pi pi-times-circle text-red text-xs"
                />
            </div>

            <FloatLabel v-if="setPasswd" class="mt-2 mb-1">
                <Password
                    ref="confirmPasswdInputArea"
                    :class="{ filled: !passwordIsInvalid(confirmPasswdInput) }"
                    id="confirm-password"
                    v-model="confirmPasswdInput"
                    :invalid="
                        confirmPasswdTouched &&
                        (passwordIsInvalid(confirmPasswdInput) || passwordsMismatch())
                    "
                    :feedback="false"
                    :pt="{ pcInputText: { root: { autocomplete: 'new-password' } } }"
                    toggle-mask
                    required
                    bellow
                    @keydown.enter="closeAndProcess"
                />
                <label for="confirm-password">{{ t('common.confirmPassword') }}</label>
            </FloatLabel>
            <div v-if="setPasswd" class="min-h-[1.2rem] flex items-center gap-1 mb-16">
                <template v-if="confirmPasswdTouched">
                    <template v-if="passwordIsInvalid(confirmPasswdInput)">
                        <i class="pi pi-times-circle text-red text-xs" />
                    </template>
                    <template v-else-if="passwordsMismatch()">
                        <i class="pi pi-times-circle text-red text-xs" />
                        <span class="text-red text-xs">{{
                            t('components.password.passwordMismatch')
                        }}</span>
                    </template>
                </template>
            </div>
        </template>

        <p v-if="submitError" class="text-red text-sm text-center mb-2 mt-[-2.5rem]">
            {{ submitError }}
        </p>
        <footer class="flex flex-col items-center place-content-between mt-4">
            <Button
                v-if="step === 1"
                class="bg-accent/80 hover:!bg-accent/100 w-full"
                @click="goNext"
                :disabled="passwordIsInvalid(currentPasswdInput) || submitting"
                :loading="submitting"
            >
                {{ t('components.password.continueButton') }}
            </Button>
            <Button
                v-if="step === 2"
                class="bg-accent/80 hover:!bg-accent/100 w-full"
                @click="closeAndProcess"
                :disabled="formIsInvalid() || submitting"
                :loading="submitting"
            >
                {{ setPasswd ? t('common.savePassword') : t('common.ok') }}
            </Button>
            <Button
                v-if="step === 2 && setPasswd && !autoFilledCurrentPasswd"
                text
                class="mt-2 text-text-color-secondary"
                @click="goBack"
            >
                ← {{ t('components.password.backButton') }}
            </Button>
            <br />
            <span
                v-if="step === 2 || !setPasswd"
                class="text-text-color-secondary text-sm underline underline-offset-2 select-none"
                v-tooltip.bottom="{
                    value: t('components.password.passwordHelp'),
                    autoHide: false,
                    escape: false,
                    hideDelay: 800,
                }"
            >
                {{ t('components.password.forgotPassword') }}
            </span>
        </footer>
    </form>
</template>

<style scoped lang="scss"></style>
