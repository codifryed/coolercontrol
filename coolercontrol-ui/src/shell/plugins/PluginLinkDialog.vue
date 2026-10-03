<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { inject, onMounted, onUnmounted, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { DynamicDialogInstance } from '@/shell/dialog'
import UiButton from '@/shell/ui/UiButton.vue'
import { OPENER_ARM_DELAY_MS, openerDocument } from '@/composables/pluginLinkOpener.ts'

const FOCUS_POLL_MS = 200

const { t } = useI18n()

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const pluginId: string = dialogRef.value.data.pluginId
const url: URL = dialogRef.value.data.url

const openButton = ref<InstanceType<typeof UiButton> | null>(null)
const opener = ref<HTMLIFrameElement | null>(null)
const openerDoc = ref('')

let armTimer: ReturnType<typeof setTimeout> | undefined
let focusPoll: ReturnType<typeof setInterval> | undefined
onMounted(() => {
    armTimer = setTimeout(() => {
        const buttonStyle = getComputedStyle(openButton.value!.$el)
        openerDoc.value = openerDocument(url, t('layout.plugins.openLink'), {
            colorScheme: document.documentElement.style.colorScheme,
            radius: buttonStyle.borderRadius,
            ringColor: buttonStyle.color,
        })
    }, OPENER_ARM_DELAY_MS)
    // The opener frame runs no script, so it cannot say its link was used. Focus resting in
    // it while the window has none means the link took the user to another tab or window.
    focusPoll = setInterval(() => {
        if (document.activeElement === opener.value && !document.hasFocus()) {
            dialogRef.value.close()
        }
    }, FOCUS_POLL_MS)
})
onUnmounted(() => {
    clearTimeout(armTimer)
    clearInterval(focusPoll)
})
</script>

<template>
    <div class="flex max-w-lg flex-col gap-4">
        <p>{{ t('layout.plugins.openLinkMessage', { plugin: pluginId }) }}</p>
        <div class="rounded-lg bg-bg-one p-3">
            <div class="break-all text-lg font-semibold">{{ url.hostname }}</div>
            <code class="select-all break-all text-sm text-text-color-secondary">{{
                url.href
            }}</code>
        </div>
        <div class="mt-2 flex justify-end gap-3">
            <UiButton variant="outline" autofocus @click="dialogRef.close()">
                {{ t('common.cancel') }}
            </UiButton>
            <!-- The button is only the look: the frame over it holds the link. The frame may
                 open a tab that is not itself sandboxed, and nothing else: no scripts. -->
            <div class="relative">
                <UiButton ref="openButton" tabindex="-1" aria-hidden="true" :disabled="!openerDoc">
                    {{ t('layout.plugins.openLink') }}
                </UiButton>
                <iframe
                    v-if="openerDoc"
                    ref="opener"
                    class="absolute inset-0 h-full w-full border-0"
                    sandbox="allow-popups allow-popups-to-escape-sandbox"
                    :srcdoc="openerDoc"
                    :title="t('layout.plugins.openLink')"
                />
            </div>
        </div>
    </div>
</template>
