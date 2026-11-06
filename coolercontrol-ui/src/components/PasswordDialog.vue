<!--
  SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { inject, nextTick, ref, watch, type Ref } from 'vue'
import type { DynamicDialogInstance } from '@/shell/dialog'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiCloseCircle, mdiContentCopy } from '@mdi/js'
import UiPasswordInput from '@/shell/ui/UiPasswordInput.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import { useI18n } from 'vue-i18n'
import { useToast } from '@/shell/toast'
import { useDeviceStore } from '@/stores/DeviceStore.ts'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const { t } = useI18n()
const toast = useToast()
const deviceStore = useDeviceStore()

const RESET_COMMANDS: ReadonlyArray<{ labelKey: string; command: string }> = [
    {
        labelKey: 'components.password.forgotPasswordPackageLabel',
        command: 'sudo coolercontrold --reset-password',
    },
    {
        labelKey: 'components.password.forgotPasswordDockerLabel',
        command: 'sudo docker exec coolercontrold coolercontrold --reset-password',
    },
    {
        labelKey: 'components.password.forgotPasswordPodmanLabel',
        command: 'sudo podman exec coolercontrold coolercontrold --reset-password',
    },
]
const showForgotPasswordHelp: Ref<boolean> = ref(false)

const toggleForgotPasswordHelp = (): void => {
    showForgotPasswordHelp.value = !showForgotPasswordHelp.value
}

const copyResetCommand = async (command: string): Promise<void> => {
    try {
        await navigator.clipboard.writeText(command)
        toast.add({
            severity: 'success',
            summary: t('components.password.forgotPasswordCommandCopied'),
            life: 1500,
        })
    } catch {
        // Clipboard write may fail (insecure context, permissions); silently
        // ignore: the command is still visible for manual selection.
    }
}

const reloadAfterReset = (): void => {
    deviceStore.reloadUI(true)
}

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
        ((currentPasswd: string, passwd: string) => Promise<string | null>) | undefined =
        dialogRef.value.data.onSubmit
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
    nextTick(() => passwdInputArea.value.focus())
}

const goBack = (): void => {
    step.value = 1
    submitError.value = null
    nextTick(() => currentPasswdInputArea.value.focus())
}

const focusConfirm = (): void => {
    if (passwordIsInvalid(passwdInput.value)) return
    nextTick(() => confirmPasswdInputArea.value.focus())
}

nextTick(async () => {
    await new Promise((resolve) => setTimeout(resolve, 300))
    if (step.value === 1) {
        currentPasswdInputArea.value.focus()
    } else {
        passwdInputArea.value.focus()
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
            <div class="mt-6 mb-1">
                <label
                    for="current-password"
                    class="mb-1 ml-1 block text-sm text-text-color-secondary"
                    >{{ t('common.currentPassword') }}</label
                >
                <UiPasswordInput
                    ref="currentPasswdInputArea"
                    id="current-password"
                    v-model="currentPasswdInput"
                    :invalid="currentPasswdTouched && passwordIsInvalid(currentPasswdInput)"
                    autocomplete="current-password"
                    required
                    @keydown.enter="goNext"
                />
            </div>
            <div class="min-h-[1.2rem] flex items-center gap-1 mb-16">
                <svg-icon
                    v-if="currentPasswdTouched && passwordIsInvalid(currentPasswdInput)"
                    type="mdi"
                    :path="mdiCloseCircle"
                    :size="12"
                    class="text-error"
                />
            </div>
        </template>

        <!-- Step 2: new password + confirm -->
        <template v-if="step === 2">
            <input type="text" autocomplete="username" value="CCAdmin" hidden aria-hidden="true" />
            <div class="mt-6 mb-1">
                <label
                    :for="setPasswd ? 'new-password' : 'password'"
                    class="mb-1 ml-1 block text-sm text-text-color-secondary"
                    >{{ setPasswd ? t('common.newPassword') : t('common.password') }}</label
                >
                <UiPasswordInput
                    ref="passwdInputArea"
                    :id="setPasswd ? 'new-password' : 'password'"
                    v-model="passwdInput"
                    :invalid="passwdTouched && passwordIsInvalid(passwdInput)"
                    :autocomplete="setPasswd ? 'new-password' : 'current-password'"
                    required
                    @keydown.enter="setPasswd ? focusConfirm() : closeAndProcess()"
                />
            </div>
            <div class="min-h-[1.2rem] flex items-center gap-1">
                <svg-icon
                    v-if="passwdTouched && passwordIsInvalid(passwdInput)"
                    type="mdi"
                    :path="mdiCloseCircle"
                    :size="12"
                    class="text-error"
                />
            </div>

            <div v-if="setPasswd" class="mt-2 mb-1">
                <label
                    for="confirm-password"
                    class="mb-1 ml-1 block text-sm text-text-color-secondary"
                    >{{ t('common.confirmPassword') }}</label
                >
                <UiPasswordInput
                    ref="confirmPasswdInputArea"
                    id="confirm-password"
                    v-model="confirmPasswdInput"
                    :invalid="
                        confirmPasswdTouched &&
                        (passwordIsInvalid(confirmPasswdInput) || passwordsMismatch())
                    "
                    autocomplete="new-password"
                    required
                    @keydown.enter="closeAndProcess"
                />
            </div>
            <div v-if="setPasswd" class="min-h-[1.2rem] flex items-center gap-1 mb-16">
                <template v-if="confirmPasswdTouched">
                    <template v-if="passwordIsInvalid(confirmPasswdInput)">
                        <svg-icon type="mdi" :path="mdiCloseCircle" :size="12" class="text-error" />
                    </template>
                    <template v-else-if="passwordsMismatch()">
                        <svg-icon type="mdi" :path="mdiCloseCircle" :size="12" class="text-error" />
                        <span class="text-error text-xs">{{
                            t('components.password.passwordMismatch')
                        }}</span>
                    </template>
                </template>
            </div>
        </template>

        <p v-if="submitError" class="text-error text-sm text-center mb-2 mt-[-2.5rem]">
            {{ submitError }}
        </p>
        <footer class="flex flex-col items-center place-content-between mt-4">
            <UiButton
                v-if="step === 1"
                class="w-full"
                @click="goNext"
                :disabled="passwordIsInvalid(currentPasswdInput) || submitting"
            >
                {{ t('components.password.continueButton') }}
            </UiButton>
            <UiButton
                v-if="step === 2"
                class="w-full"
                @click="closeAndProcess"
                :disabled="formIsInvalid() || submitting"
            >
                {{ setPasswd ? t('common.savePassword') : t('common.ok') }}
            </UiButton>
            <UiButton
                v-if="step === 2 && setPasswd && !autoFilledCurrentPasswd"
                variant="ghost"
                class="mt-2 text-text-color-secondary"
                @click="goBack"
            >
                ← {{ t('components.password.backButton') }}
            </UiButton>
            <br />
            <span
                v-if="!autoFilledCurrentPasswd"
                class="text-text-color-secondary text-sm underline underline-offset-2 select-none cursor-pointer"
                role="button"
                tabindex="0"
                @click="toggleForgotPasswordHelp"
                @keydown.enter.prevent="toggleForgotPasswordHelp"
                @keydown.space.prevent="toggleForgotPasswordHelp"
            >
                {{ t('components.password.forgotPassword') }}
            </span>
            <div
                v-if="showForgotPasswordHelp && !autoFilledCurrentPasswd"
                class="w-full mt-3 flex flex-col gap-2"
            >
                <p class="text-text-color-secondary text-sm">
                    {{ t('components.password.forgotPasswordHelpIntro') }}
                </p>
                <div v-for="reset in RESET_COMMANDS" :key="reset.command">
                    <p class="text-text-color-secondary text-xs mb-1">
                        {{ t(reset.labelKey) }}
                    </p>
                    <div
                        class="flex items-center gap-2 bg-bg-two border border-border-one rounded px-2 py-1"
                    >
                        <code class="flex-1 font-mono text-sm select-all break-all">{{
                            reset.command
                        }}</code>
                        <UiButton
                            variant="ghost"
                            size="icon"
                            :aria-label="t('components.password.forgotPasswordCopyCommand')"
                            v-tooltip.top="t('components.password.forgotPasswordCopyCommand')"
                            @click="copyResetCommand(reset.command)"
                        >
                            <svg-icon type="mdi" :path="mdiContentCopy" :size="18" />
                        </UiButton>
                    </div>
                </div>
                <p class="text-text-color-secondary text-xs">
                    {{ t('components.password.forgotPasswordContainerNote') }}
                </p>
                <UiButton class="mt-1" @click="reloadAfterReset">
                    {{ t('components.password.forgotPasswordReloadButton') }}
                </UiButton>
            </div>
        </footer>
    </form>
</template>

<style scoped lang="scss"></style>
