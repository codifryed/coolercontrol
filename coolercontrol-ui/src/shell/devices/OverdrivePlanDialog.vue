<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { computed, inject, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { DynamicDialogInstance } from '@/shell/dialog'
import UiButton from '@/shell/ui/UiButton.vue'
import {
    type AmdGpuOverdrivePlan,
    type OverdriveAction,
    type OverdriveMessage,
    overdrivePlanIntro,
    overdrivePlanRecovery,
    overdrivePlanTakesTime,
} from '@/models/AmdGpuOverdrive.ts'

// Shows exactly what enabling or disabling runs as root before it runs. Closes with
// `{ rebuild }` when the user goes ahead, and with nothing when they back out.
const { t } = useI18n()

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const plan: AmdGpuOverdrivePlan = dialogRef.value.data.plan
const action: OverdriveAction = dialogRef.value.data.action
const enabling = action === 'enable'

const translate = (message: OverdriveMessage | undefined) =>
    message != null ? t(message.key, message.params) : undefined
const intro = computed(() => translate(overdrivePlanIntro(plan, action)))
const recovery = computed(() => translate(overdrivePlanRecovery(plan, action)))
const goLabel = computed(() => {
    const rebuild = plan.route === 'rebuild'
    if (enabling) {
        return rebuild
            ? t('components.deviceExtensionSettings.overdriveEnableAndRebuild')
            : t('components.deviceExtensionSettings.overdriveEnable')
    }
    return rebuild
        ? t('components.deviceExtensionSettings.overdriveDisableAndRebuild')
        : t('components.deviceExtensionSettings.overdriveDisable')
})
</script>

<template>
    <div class="flex max-w-xl flex-col gap-4">
        <template v-if="plan.problem">
            <p class="text-error">{{ plan.problem }}</p>
            <div class="mt-2 flex justify-end">
                <UiButton variant="outline" autofocus @click="dialogRef.close()">
                    {{ t('common.ok') }}
                </UiButton>
            </div>
        </template>
        <template v-else>
            <p>{{ intro }}</p>
            <code
                v-if="plan.command"
                class="select-all break-all rounded-lg bg-bg-one p-3 text-sm"
                >{{ plan.command }}</code
            >
            <p v-if="overdrivePlanTakesTime(plan)">
                {{ t('components.deviceExtensionSettings.overdrivePlanDuration') }}
            </p>
            <p v-if="recovery" class="text-sm text-text-color-secondary">{{ recovery }}</p>
            <p v-if="enabling" class="text-sm text-text-color-secondary">
                {{ t('components.deviceExtensionSettings.overdrivePlanBugReports') }}
            </p>
            <div class="mt-2 flex flex-wrap justify-end gap-3">
                <UiButton variant="outline" autofocus @click="dialogRef.close()">
                    {{ t('common.cancel') }}
                </UiButton>
                <UiButton
                    v-if="enabling && plan.route === 'rebuild'"
                    variant="outline"
                    @click="dialogRef.close({ rebuild: false })"
                >
                    {{ t('components.deviceExtensionSettings.overdriveConfigOnly') }}
                </UiButton>
                <UiButton @click="dialogRef.close({ rebuild: true })">
                    {{ goLabel }}
                </UiButton>
            </div>
        </template>
    </div>
</template>
