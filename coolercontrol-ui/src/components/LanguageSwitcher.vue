<template>
    <Dropdown
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
                        : '选择语言'
                }}
            </div>
        </template>
    </Dropdown>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import Dropdown from 'primevue/dropdown'
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
