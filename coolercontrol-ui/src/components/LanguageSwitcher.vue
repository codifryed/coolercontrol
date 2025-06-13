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

<template>
    <Select
        v-model="currentLocale"
        :options="localeOptions"
        optionLabel="name"
        optionValue="code"
        @change="changeLocale"
        class="w-full h-[2.375rem]"
        :loading="isLoading"
        :pt="{
            input: { class: 'text-center' },
            trigger: { class: 'flex justify-center items-center' },
            label: { class: 'text-center w-full' },
            panel: { class: 'border-2 border-border-one rounded-lg shadow-lg bg-bg-one' },
        }"
    >
        <template #value="slotProps">
            <div class="flex justify-center items-center w-full h-full">
                {{
                    slotProps.value
                        ? localeOptions.find((option) => option.code === slotProps.value)?.name
                        : t('layout.settings.selectLanguage')
                }}
            </div>
        </template>
    </Select>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import Select from 'primevue/select'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'

const { locale, t } = useI18n()
const confirm = useConfirm()
const toast = useToast()
const currentLocale = ref(localStorage.getItem('locale') || 'en')
const isLoading = ref(false)

const localeOptions = [
    { name: 'English', code: 'en' },
    { name: '中文（简体）', code: 'zh' },
    { name: '中文（繁體）', code: 'zh-tw' },
    { name: '日本語', code: 'ja' },
    { name: 'Русский', code: 'ru' },
    { name: 'Deutsch', code: 'de' },
    { name: 'Français', code: 'fr' },
    { name: 'Español', code: 'es' },
    { name: 'العربية', code: 'ar' },
    { name: 'Português', code: 'pt' },
    { name: 'हिन्दी', code: 'hi' },
]

// Ensure currentLocale matches locale when component is mounted
onMounted(() => {
    // If they don't match, use locale as the source of truth
    if (currentLocale.value !== locale.value) {
        currentLocale.value = locale.value
    }
})

function changeLocale(event: { value: string }) {
    const selectedLocale = event.value

    // We should actually compare the target language with the current locale value, not currentLocale
    if (selectedLocale === locale.value) {
        return
    }

    // Show confirmation dialog
    confirm.require({
        message: t('layout.settings.languageChangeConfirmMessage'),
        header: t('layout.settings.languageChangeConfirm'),
        icon: 'pi pi-language',
        acceptLabel: t('common.ok'),
        rejectLabel: t('common.cancel'),
        accept: () => {
            try {
                // Set loading state
                isLoading.value = true

                // Update language settings
                locale.value = selectedLocale
                localStorage.setItem('locale', selectedLocale)

                // Apply new language setting to HTML element
                document.querySelector('html')?.setAttribute('lang', selectedLocale)

                // Force refresh application state to ensure all components dependent on i18n are updated
                window.dispatchEvent(
                    new CustomEvent('language-changed', { detail: selectedLocale }),
                )

                // Show success toast instead of refreshing the page
                toast.add({
                    severity: 'success',
                    summary: t('common.success'),
                    detail: t('layout.settings.languageChangeSuccess'),
                    life: 3000,
                })

                // Reset loading state
                isLoading.value = false
            } catch (error) {
                isLoading.value = false
                // Show error toast if language switch fails
                toast.add({
                    severity: 'error',
                    summary: t('common.error'),
                    detail: t('layout.settings.languageChangeError'),
                    life: 4000,
                })
            }
        },
        reject: () => {
            // User canceled the operation, restore original selection
            currentLocale.value = locale.value
        },
    })
}

// Watch for language changes
watch(
    () => locale.value,
    (newLocale) => {
        currentLocale.value = newLocale
        document.querySelector('html')?.setAttribute('lang', newLocale)
    },
)
</script>
