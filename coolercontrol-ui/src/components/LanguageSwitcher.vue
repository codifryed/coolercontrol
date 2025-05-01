<template>
    <Dropdown
        v-model="currentLocale"
        :options="localeOptions"
        optionLabel="name"
        optionValue="code"
        @change="changeLocale"
        class="w-32"
        :loading="isLoading"
    />
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import Dropdown from 'primevue/dropdown'
import { useConfirm } from 'primevue/useconfirm'

const { locale, t } = useI18n()
const confirm = useConfirm()
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

                // Use a more forceful page refresh method
                setTimeout(() => {
                    // Ensure a full reload without using cache
                    window.location.href =
                        window.location.href.split('#')[0] +
                        (window.location.search ? window.location.search : '?') +
                        (window.location.search ? '&' : '') +
                        '_t=' +
                        new Date().getTime()
                }, 300)
            } catch (error) {
                isLoading.value = false
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
