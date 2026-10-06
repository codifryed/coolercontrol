<!--
  SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiContentCopy, mdiLinkVariant } from '@mdi/js'
import { inject, Ref } from 'vue'
import type { DynamicDialogInstance } from '@/shell/dialog'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'
import { useToast } from '@/shell/toast'
import { PluginDto } from '@/models/Plugins.ts'
import { validatePluginLinkForUi } from '@/composables/pluginLinkValidation.ts'
import { usePluginLinks } from '@/composables/usePluginLinks.ts'

const deviceStore = useDeviceStore()
const toast = useToast()
const { t } = useI18n()

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const plugin: PluginDto = dialogRef.value.data.plugin
const isManaged: boolean = dialogRef.value.data.isManaged ?? false
const isPluginLinkOpenable =
    validatePluginLinkForUi(plugin.url, deviceStore.daemonClient.daemonURL) != null
const pluginLinks = usePluginLinks()

const copyCommand = (text: string): void => {
    if (navigator.clipboard?.writeText) {
        navigator.clipboard.writeText(text).catch(() => fallbackCopy(text))
    } else {
        fallbackCopy(text)
    }
    toast.add({
        severity: 'info',
        summary: t('layout.plugins.commandCopied'),
        life: 2000,
    })
}

const fallbackCopy = (text: string): void => {
    const textarea = document.createElement('textarea')
    textarea.value = text
    textarea.style.position = 'fixed'
    textarea.style.opacity = '0'
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
}
</script>

<template>
    <div class="flex flex-col min-w-[30vw] p-2">
        <div v-if="plugin.description" class="text-wrap italic text-text-color-secondary mb-4">
            {{ plugin.description }}
        </div>
        <table class="bg-bg-two rounded-lg w-full">
            <tbody>
                <tr class="border-b border-border-one">
                    <td class="py-3 px-4 font-semibold w-40">
                        {{ t('layout.plugins.type') }}
                    </td>
                    <td class="py-3 px-4">{{ plugin.service_type }}</td>
                </tr>
                <tr v-if="plugin.version" class="border-b border-border-one">
                    <td class="py-3 px-4 font-semibold">
                        {{ t('views.appInfo.version') }}
                    </td>
                    <td class="py-3 px-4">{{ plugin.version }}</td>
                </tr>
                <tr v-if="plugin.address" class="border-b border-border-one">
                    <td class="py-3 px-4 font-semibold">
                        {{ t('layout.plugins.address') }}
                    </td>
                    <td class="py-3 px-4">
                        <code>{{ plugin.address }}</code>
                    </td>
                </tr>
                <tr class="border-b border-border-one">
                    <td class="py-3 px-4 font-semibold">
                        {{ t('layout.plugins.privileges') }}
                    </td>
                    <td class="py-3 px-4" :class="{ 'font-bold': plugin.privileged }">
                        {{
                            plugin.privileged
                                ? t('layout.settings.plugins.privileged')
                                : t('layout.settings.plugins.restricted')
                        }}
                    </td>
                </tr>
                <tr v-if="plugin.url" :class="{ 'border-b border-border-one': isManaged }">
                    <td class="py-3 px-4 font-semibold">{{ t('layout.plugins.url') }}</td>
                    <td class="py-3 px-4">
                        <button
                            v-if="isPluginLinkOpenable"
                            type="button"
                            class="inline-flex items-center gap-1 underline"
                            @click="pluginLinks.requestLink(plugin.id, plugin.url, true)"
                        >
                            <svg-icon
                                type="mdi"
                                :path="mdiLinkVariant"
                                :size="deviceStore.getREMSize(1)"
                            />
                            {{ t('layout.settings.plugins.pluginUrl') }}
                        </button>
                        <!-- Not a link the UI may open, so it is shown rather than followed. -->
                        <code v-else class="select-all break-all">{{ plugin.url }}</code>
                    </td>
                </tr>
                <tr v-if="isManaged">
                    <td class="py-3 px-4 font-semibold align-top">
                        {{ t('layout.plugins.serviceLogs') }}
                    </td>
                    <td class="py-3 px-4 space-y-1">
                        <div class="flex items-center gap-1">
                            <span class="text-xs text-text-color-secondary">systemd:</span>
                            <code class="ml-1 select-all"
                                >journalctl -f -u cc-plugin-{{ plugin.id }}</code
                            >
                            <svg-icon
                                type="mdi"
                                :path="mdiContentCopy"
                                :size="14"
                                class="cursor-pointer opacity-60 hover:opacity-100"
                                @click="copyCommand(`journalctl -f -u cc-plugin-${plugin.id}`)"
                            />
                        </div>
                        <div class="flex items-center gap-1">
                            <span class="text-xs text-text-color-secondary">OpenRC:</span>
                            <code class="ml-1 select-all"
                                >grep cc-plugin-{{ plugin.id }} /var/log/messages</code
                            >
                            <svg-icon
                                type="mdi"
                                :path="mdiContentCopy"
                                :size="14"
                                class="cursor-pointer opacity-60 hover:opacity-100"
                                @click="
                                    copyCommand(`grep cc-plugin-${plugin.id} /var/log/messages`)
                                "
                            />
                        </div>
                    </td>
                </tr>
            </tbody>
        </table>
    </div>
</template>

<style scoped lang="scss"></style>
